use base64::{engine::general_purpose::STANDARD, Engine};
// Access simplicity-lang through simplicityhl's re-export so we don't widen
// the dep graph (and risk pulling in something that doesn't target wasm32).
use simplicityhl::simplicity::{
    dag::{DagLike, MaxSharing},
    encode::encode_hash,
    encode_natural, encode_value,
    jet::Elements,
    jet::Jet as JetEncode,
    node::{Commit, CommitNode, Inner},
    BitIter, BitWriter, Cmr, FailEntropy, Word,
};
use std::collections::{BTreeSet, HashMap};
use std::io;
use wasm_bindgen::prelude::*;

/// Canonicalize a compiled Simplicity program supplied as a base64 string.
///
/// Returns a JSON object:
/// - success: `{"ok":true,"program":"<base64>","cmr":"<hex>","canonical_prefix":"<hex>","canonical_prefix_b64":"<base64>"}`
/// - failure: `{"ok":false,"error":"<message>"}`
#[wasm_bindgen]
pub fn canonicalize(b64: &str) -> String {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();

    match do_canonicalize(b64.trim()) {
        Ok(out) => serde_json::json!({
            "ok": true,
            "program": out.program,
            "cmr": out.cmr,
            "canonical_prefix": out.canonical_prefix,
            "canonical_prefix_b64": out.canonical_prefix_b64,
            "jets": out.jets,
            "readable": out.readable,
        })
        .to_string(),
        Err(e) => serde_json::json!({ "ok": false, "error": e }).to_string(),
    }
}

struct Out {
    program: String,
    cmr: String,
    canonical_prefix: String,
    canonical_prefix_b64: String,
    jets: Vec<String>,
    readable: String,
}

fn do_canonicalize(b64: &str) -> Result<Out, String> {
    let bytes = STANDARD.decode(b64)
        .map_err(|e| format!("Failed to decode base64: {e}"))?;
    let bits = BitIter::from(bytes.as_slice());
    let node = CommitNode::<Elements>::decode(bits)
        .map_err(|e| format!("Failed to decode program: {e}"))?;

    let cmr = compute_canonical_cmr(&node).to_string();
    let program_bytes = canonical_encode_to_vec(&node);
    let program_b64 = STANDARD.encode(&program_bytes);

    let prefix_bytes = &program_bytes[..program_bytes.len().min(8)];
    let canonical_prefix: String = prefix_bytes.iter().map(|b| format!("{b:02x}")).collect();
    let canonical_prefix_b64 = STANDARD.encode(prefix_bytes);

    let jets = collect_jets(&node);
    let readable = disassemble(&node);

    Ok(Out {
        program: program_b64,
        cmr,
        canonical_prefix,
        canonical_prefix_b64,
        jets,
        readable,
    })
}

fn collect_jets(root: &CommitNode<Elements>) -> Vec<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for data in root.post_order_iter::<MaxSharing<Commit<Elements>>>() {
        if let Inner::Jet(j) = data.node.inner() {
            seen.insert(format!("{j}"));
        }
    }
    seen.into_iter().collect()
}

fn disassemble(root: &CommitNode<Elements>) -> String {
    let nodes: Vec<_> = root.post_order_iter::<MaxSharing<Commit<Elements>>>().collect();
    let mut lines = Vec::with_capacity(nodes.len());
    for (i, data) in nodes.iter().enumerate() {
        let desc = match data.node.inner() {
            Inner::Iden           => "iden".to_string(),
            Inner::Unit           => "unit".to_string(),
            Inner::InjL(_)        => format!("injl {}", data.left_index.unwrap()),
            Inner::InjR(_)        => format!("injr {}", data.left_index.unwrap()),
            Inner::Take(_)        => format!("take {}", data.left_index.unwrap()),
            Inner::Drop(_)        => format!("drop {}", data.left_index.unwrap()),
            Inner::Comp(_, _)     => format!("comp {} {}", data.left_index.unwrap(), data.right_index.unwrap()),
            Inner::Case(_, _)     => format!("case {} {}", data.left_index.unwrap(), data.right_index.unwrap()),
            Inner::Pair(_, _)     => format!("pair {} {}", data.left_index.unwrap(), data.right_index.unwrap()),
            Inner::Disconnect(_, _) => format!("disconnect {}", data.left_index.unwrap()),
            Inner::AssertL(_, _)  => format!("assertl {} _", data.left_index.unwrap()),
            Inner::AssertR(_, _)  => format!("assertr _ {}", data.left_index.unwrap()),
            Inner::Witness(_)     => "witness".to_string(),
            Inner::Fail(_)        => "fail".to_string(),
            Inner::Jet(j)         => format!("jet::{j}"),
            Inner::Word(w)        => format!("word(2^{})", w.n()),
        };
        lines.push(format!("{i}: {desc}"));
    }
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Canonical CMR (ported from cli/src/canonicalize.rs)
// ---------------------------------------------------------------------------

fn compute_canonical_cmr(node: &CommitNode<Elements>) -> Cmr {
    let mut cmrs: Vec<Cmr> = Vec::new();
    for data in node.post_order_iter::<MaxSharing<Commit<Elements>>>() {
        let lc = data.left_index.map(|i| cmrs[i]);
        let rc = data.right_index.map(|i| cmrs[i]);
        let canonical = match data.node.inner() {
            Inner::Iden => Cmr::iden(),
            Inner::Unit => Cmr::unit(),
            Inner::InjL(_) => Cmr::injl(lc.unwrap()),
            Inner::InjR(_) => Cmr::injr(lc.unwrap()),
            Inner::Take(_) => Cmr::take(lc.unwrap()),
            Inner::Drop(_) => Cmr::drop(lc.unwrap()),
            Inner::Comp(_, _) => Cmr::comp(lc.unwrap(), rc.unwrap()),
            Inner::Case(_, _) => Cmr::case(lc.unwrap(), rc.unwrap()),
            Inner::Pair(_, _) => Cmr::pair(lc.unwrap(), rc.unwrap()),
            Inner::Disconnect(_, _) => Cmr::disconnect(lc.unwrap()),
            Inner::AssertL(_, _) => Cmr::case(lc.unwrap(), Cmr::unit()),
            Inner::AssertR(_, _) => Cmr::case(Cmr::unit(), lc.unwrap()),
            Inner::Witness(_) => Cmr::witness(),
            Inner::Fail(entropy) => Cmr::fail(*entropy),
            Inner::Jet(jet) => Cmr::jet(*jet),
            Inner::Word(w) => Cmr::const_word(&zero_word(w.n())),
        };
        cmrs.push(canonical);
    }
    cmrs.pop().expect("CommitNode is non-empty")
}

// ---------------------------------------------------------------------------
// Canonical program encoder (ported from cli/src/canonicalize.rs)
// ---------------------------------------------------------------------------

enum CanonItem {
    Comp(usize, usize),
    Case(usize, usize),
    Pair(usize, usize),
    InjL(usize),
    InjR(usize),
    Take(usize),
    Drop(usize),
    Disconnect(usize),
    Hidden,
    Iden,
    Unit,
    Fail(FailEntropy),
    Witness,
    Jet(Elements),
    Word(u32),
}

struct CanonEncoder {
    items: Vec<CanonItem>,
    node_idx: HashMap<usize, usize>,
    canon_hidden: Option<usize>,
}

impl CanonEncoder {
    fn new() -> Self {
        CanonEncoder { items: Vec::new(), node_idx: HashMap::new(), canon_hidden: None }
    }

    fn get_or_add_hidden(&mut self) -> usize {
        match self.canon_hidden {
            Some(idx) => idx,
            None => {
                let idx = self.items.len();
                self.items.push(CanonItem::Hidden);
                self.canon_hidden = Some(idx);
                idx
            }
        }
    }

    fn visit(&mut self, node: &CommitNode<Elements>) -> usize {
        let ptr = node as *const _ as usize;
        if let Some(&idx) = self.node_idx.get(&ptr) {
            return idx;
        }
        let idx = match node.inner() {
            Inner::Comp(l, r) => {
                let li = self.visit(l.as_ref());
                let ri = self.visit(r.as_ref());
                let i = self.items.len(); self.items.push(CanonItem::Comp(li, ri)); i
            }
            Inner::Case(l, r) => {
                let li = self.visit(l.as_ref());
                let ri = self.visit(r.as_ref());
                let i = self.items.len(); self.items.push(CanonItem::Case(li, ri)); i
            }
            Inner::Pair(l, r) => {
                let li = self.visit(l.as_ref());
                let ri = self.visit(r.as_ref());
                let i = self.items.len(); self.items.push(CanonItem::Pair(li, ri)); i
            }
            Inner::AssertL(l, _) => {
                let li = self.visit(l.as_ref());
                let hi = self.get_or_add_hidden();
                let i = self.items.len(); self.items.push(CanonItem::Case(li, hi)); i
            }
            Inner::AssertR(_, r) => {
                let hi = self.get_or_add_hidden();
                let ri = self.visit(r.as_ref());
                let i = self.items.len(); self.items.push(CanonItem::Case(hi, ri)); i
            }
            Inner::InjL(c) => {
                let ci = self.visit(c.as_ref());
                let i = self.items.len(); self.items.push(CanonItem::InjL(ci)); i
            }
            Inner::InjR(c) => {
                let ci = self.visit(c.as_ref());
                let i = self.items.len(); self.items.push(CanonItem::InjR(ci)); i
            }
            Inner::Take(c) => {
                let ci = self.visit(c.as_ref());
                let i = self.items.len(); self.items.push(CanonItem::Take(ci)); i
            }
            Inner::Drop(c) => {
                let ci = self.visit(c.as_ref());
                let i = self.items.len(); self.items.push(CanonItem::Drop(ci)); i
            }
            Inner::Disconnect(l, _) => {
                let li = self.visit(l.as_ref());
                let i = self.items.len(); self.items.push(CanonItem::Disconnect(li)); i
            }
            Inner::Iden    => { let i = self.items.len(); self.items.push(CanonItem::Iden); i }
            Inner::Unit    => { let i = self.items.len(); self.items.push(CanonItem::Unit); i }
            Inner::Fail(e) => { let i = self.items.len(); self.items.push(CanonItem::Fail(*e)); i }
            Inner::Witness(_) => { let i = self.items.len(); self.items.push(CanonItem::Witness); i }
            Inner::Jet(j)  => { let i = self.items.len(); self.items.push(CanonItem::Jet(*j)); i }
            Inner::Word(w) => { let i = self.items.len(); self.items.push(CanonItem::Word(w.n())); i }
        };
        self.node_idx.insert(ptr, idx);
        idx
    }
}

fn encode_items<W: io::Write>(items: &[CanonItem], w: &mut BitWriter<W>) -> io::Result<()> {
    encode_natural(items.len(), w)?;
    for (i, item) in items.iter().enumerate() {
        match item {
            CanonItem::Comp(l, r)       => { w.write_bits_be(0b00000, 5)?; encode_natural(i-l,w)?; encode_natural(i-r,w)?; }
            CanonItem::Case(l, r)       => { w.write_bits_be(0b00001, 5)?; encode_natural(i-l,w)?; encode_natural(i-r,w)?; }
            CanonItem::Pair(l, r)       => { w.write_bits_be(0b00010, 5)?; encode_natural(i-l,w)?; encode_natural(i-r,w)?; }
            CanonItem::InjL(c)          => { w.write_bits_be(0b00100, 5)?; encode_natural(i-c,w)?; }
            CanonItem::InjR(c)          => { w.write_bits_be(0b00101, 5)?; encode_natural(i-c,w)?; }
            CanonItem::Take(c)          => { w.write_bits_be(0b00110, 5)?; encode_natural(i-c,w)?; }
            CanonItem::Drop(c)          => { w.write_bits_be(0b00111, 5)?; encode_natural(i-c,w)?; }
            CanonItem::Disconnect(c)    => { w.write_bits_be(0b01011, 5)?; encode_natural(i-c,w)?; }
            CanonItem::Hidden           => { w.write_bits_be(0b0110, 4)?; encode_hash(&[0u8;32], w)?; }
            CanonItem::Iden             => { w.write_bits_be(0b01000, 5)?; }
            CanonItem::Unit             => { w.write_bits_be(0b01001, 5)?; }
            CanonItem::Fail(entropy)    => { w.write_bits_be(0b01010, 5)?; encode_hash(entropy.as_ref(), w)?; }
            CanonItem::Witness          => { w.write_bits_be(0b0111, 4)?; }
            CanonItem::Jet(j)           => { w.write_bit(true)?; w.write_bit(true)?; j.encode(w)?; }
            CanonItem::Word(n)          => {
                w.write_bit(true)?; w.write_bit(false)?;
                encode_natural(1 + *n as usize, w)?;
                encode_value(zero_word(*n).as_value(), w)?;
            }
        }
    }
    Ok(())
}

fn canonical_encode_to_vec(root: &CommitNode<Elements>) -> Vec<u8> {
    let mut encoder = CanonEncoder::new();
    encoder.visit(root);
    let mut bytes = Vec::new();
    let mut w = BitWriter::new(&mut bytes);
    encode_items(&encoder.items, &mut w).expect("writing to Vec never fails");
    w.flush_all().expect("flushing Vec never fails");
    bytes
}

fn zero_word(n: u32) -> Word {
    match n {
        0 => Word::u1(0),
        1 => Word::u2(0),
        2 => Word::u4(0),
        3 => Word::u8(0),
        4 => Word::u16(0),
        5 => Word::u32(0),
        6 => Word::u64(0),
        7 => Word::u128(0),
        8 => Word::u256([0u8; 32]),
        9 => Word::u512([0u8; 64]),
        _ => {
            let half = zero_word(n - 1);
            half.product(zero_word(n - 1))
                .expect("same-sized zero words can always be combined")
        }
    }
}
