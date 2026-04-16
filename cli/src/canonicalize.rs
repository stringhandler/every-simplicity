use anyhow::{Context, Result};
use simplicity::{
    dag::{DagLike, MaxSharing},
    encode::encode_hash,
    encode_natural, encode_value,
    jet::Elements,
    jet::Jet as JetEncode,
    node::{Commit, CommitNode, Inner},
    BitIter, BitWriter, Cmr, FailEntropy, Word,
};
use std::collections::HashMap;
use std::io;

// ---------------------------------------------------------------------------
// CMR helpers
// ---------------------------------------------------------------------------

/// Decode a Simplicity program from a base64 string and return its canonical CMR as hex.
///
/// The compiled `.simb` format and `simc` output are base64-encoded CommitNodes.
pub fn canonical_cmr_from_base64(b64: &str) -> Result<String> {
    let node = CommitNode::<Elements>::from_str(b64)
        .context("failed to decode Simplicity CommitNode from base64")?;
    Ok(compute_canonical_cmr(&node).to_string())
}

/// Decode a Simplicity program from raw bytes and return its canonical CMR as hex.
#[allow(dead_code)]
pub fn canonical_cmr_from_bytes(bytes: &[u8]) -> Result<String> {
    let bits = BitIter::from(bytes);
    let node = CommitNode::<Elements>::decode(bits)
        .context("failed to decode Simplicity CommitNode")?;
    Ok(compute_canonical_cmr(&node).to_string())
}

/// Compute the canonical CMR of an already-decoded [`CommitNode`].
///
/// Walks the DAG in post-order and recomputes the CMR with:
/// - Every `Word` node replaced by an all-zeros word of the same bit-width
/// - Every hidden-branch CMR in `AssertL`/`AssertR` replaced with `Cmr::unit()`
///
/// Programs that share the same template but differ only in baked-in constants
/// (e.g. a public key parameter) will produce the same canonical CMR.
pub fn compute_canonical_cmr(node: &CommitNode<Elements>) -> Cmr {
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
            // AssertL(child, hidden_right_cmr): replace hidden CMR with unit.
            Inner::AssertL(_, _) => Cmr::case(lc.unwrap(), Cmr::unit()),
            // AssertR(hidden_left_cmr, child): sole DAG child is treated as left in iteration.
            Inner::AssertR(_, _) => Cmr::case(Cmr::unit(), lc.unwrap()),
            Inner::Witness(_) => Cmr::witness(),
            Inner::Fail(entropy) => Cmr::fail(*entropy),
            Inner::Jet(jet) => Cmr::jet(*jet),
            // Replace word value with all-zeros of the same bit-width.
            Inner::Word(w) => Cmr::const_word(&zero_word(w.n())),
        };
        cmrs.push(canonical);
    }

    cmrs.pop().expect("CommitNode is non-empty")
}

// ---------------------------------------------------------------------------
// Prefix helpers
// ---------------------------------------------------------------------------

/// Return the first 16 hex characters of the compiled program bytes.
///
/// The compiled bytes come from the base64 `.simb` file.  This prefix
/// uniquely identifies the specific instantiation of the program (including
/// any baked-in constants such as public keys).
pub fn program_prefix_from_base64(b64: &str) -> Result<String> {
    let node = CommitNode::<Elements>::from_str(b64)
        .context("failed to decode Simplicity CommitNode from base64")?;
    let bytes = node.to_vec_without_witness();
    Ok(bytes_to_hex_prefix(&bytes))
}

/// Return the first 16 hex characters of the *canonical* program bytes.
///
/// The canonical program has all `Word` values zeroed and all pruned-branch
/// CMRs (`AssertL`/`AssertR`) zeroed, making it the same for every program
/// that shares the same template but differs only in baked-in constants.
/// This prefix is useful for matching on-chain programs back to a known template.
pub fn canonical_prefix_from_base64(b64: &str) -> Result<String> {
    let node = CommitNode::<Elements>::from_str(b64)
        .context("failed to decode Simplicity CommitNode from base64")?;
    let bytes = canonical_encode_to_vec(&node);
    Ok(bytes_to_hex_prefix(&bytes))
}

fn bytes_to_hex_prefix(bytes: &[u8]) -> String {
    bytes[..bytes.len().min(8)]
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

// ---------------------------------------------------------------------------
// Canonical program encoder
// ---------------------------------------------------------------------------

/// Items in the canonical post-order encoding sequence.
///
/// Mirrors the Simplicity bit encoding format but with Word values zeroed and
/// AssertL/AssertR hidden-branch CMRs replaced by a single shared zero node.
enum CanonItem {
    /// Composition — opcode 0b00000
    Comp(usize, usize),
    /// Case (also used for canonical AssertL/AssertR) — opcode 0b00001
    Case(usize, usize),
    /// Pair — opcode 0b00010
    Pair(usize, usize),
    /// InjL — opcode 0b00100
    InjL(usize),
    /// InjR — opcode 0b00101
    InjR(usize),
    /// Take — opcode 0b00110
    Take(usize),
    /// Drop — opcode 0b00111
    Drop(usize),
    /// Disconnect (commit form, NoDisconnect right) — opcode 0b01011
    Disconnect(usize),
    /// Canonical hidden node — opcode 0b0110 followed by 32 zero bytes
    Hidden,
    /// Iden — opcode 0b01000
    Iden,
    /// Unit — opcode 0b01001
    Unit,
    /// Fail — opcode 0b01010 followed by 64-byte entropy (kept as-is)
    Fail(FailEntropy),
    /// Witness — opcode 0b0111
    Witness,
    /// Jet — opcode 0b11 followed by jet encoding
    Jet(Elements),
    /// Word (zeroed) — opcode 0b10, encode_natural(1+n), then zero bits
    Word(u32),
}

/// State for the canonical DAG traversal.
struct CanonEncoder {
    items: Vec<CanonItem>,
    /// raw pointer of CommitNode data → encoding index
    node_idx: HashMap<usize, usize>,
    /// Encoding index of the single shared canonical hidden node, if created.
    canon_hidden: Option<usize>,
}

impl CanonEncoder {
    fn new() -> Self {
        CanonEncoder {
            items: Vec::new(),
            node_idx: HashMap::new(),
            canon_hidden: None,
        }
    }

    /// Return the encoding index of the canonical hidden (zero-CMR) node,
    /// creating it in the items list on first call.
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

    /// Post-order visit of `node`, recording items for canonical encoding.
    /// Returns the encoding index assigned to this node.
    fn visit(&mut self, node: &CommitNode<Elements>) -> usize {
        let ptr = node as *const _ as usize;
        if let Some(&idx) = self.node_idx.get(&ptr) {
            return idx;
        }

        let idx = match node.inner() {
            Inner::Comp(l, r) => {
                let li = self.visit(l.as_ref());
                let ri = self.visit(r.as_ref());
                let idx = self.items.len();
                self.items.push(CanonItem::Comp(li, ri));
                idx
            }
            Inner::Case(l, r) => {
                let li = self.visit(l.as_ref());
                let ri = self.visit(r.as_ref());
                let idx = self.items.len();
                self.items.push(CanonItem::Case(li, ri));
                idx
            }
            Inner::Pair(l, r) => {
                let li = self.visit(l.as_ref());
                let ri = self.visit(r.as_ref());
                let idx = self.items.len();
                self.items.push(CanonItem::Pair(li, ri));
                idx
            }
            // AssertL(child, rcmr): left child + canonical hidden right
            Inner::AssertL(l, _rcmr) => {
                let li = self.visit(l.as_ref());
                let hi = self.get_or_add_hidden();
                let idx = self.items.len();
                self.items.push(CanonItem::Case(li, hi));
                idx
            }
            // AssertR(lcmr, child): canonical hidden left + right child
            Inner::AssertR(_lcmr, r) => {
                let hi = self.get_or_add_hidden();
                let ri = self.visit(r.as_ref());
                let idx = self.items.len();
                self.items.push(CanonItem::Case(hi, ri));
                idx
            }
            Inner::InjL(c) => {
                let ci = self.visit(c.as_ref());
                let idx = self.items.len();
                self.items.push(CanonItem::InjL(ci));
                idx
            }
            Inner::InjR(c) => {
                let ci = self.visit(c.as_ref());
                let idx = self.items.len();
                self.items.push(CanonItem::InjR(ci));
                idx
            }
            Inner::Take(c) => {
                let ci = self.visit(c.as_ref());
                let idx = self.items.len();
                self.items.push(CanonItem::Take(ci));
                idx
            }
            Inner::Drop(c) => {
                let ci = self.visit(c.as_ref());
                let idx = self.items.len();
                self.items.push(CanonItem::Drop(ci));
                idx
            }
            // Disconnect with NoDisconnect right: unary in commit encoding
            Inner::Disconnect(l, _nd) => {
                let li = self.visit(l.as_ref());
                let idx = self.items.len();
                self.items.push(CanonItem::Disconnect(li));
                idx
            }
            Inner::Iden => {
                let idx = self.items.len();
                self.items.push(CanonItem::Iden);
                idx
            }
            Inner::Unit => {
                let idx = self.items.len();
                self.items.push(CanonItem::Unit);
                idx
            }
            Inner::Fail(e) => {
                let idx = self.items.len();
                self.items.push(CanonItem::Fail(*e));
                idx
            }
            Inner::Witness(_) => {
                let idx = self.items.len();
                self.items.push(CanonItem::Witness);
                idx
            }
            Inner::Jet(j) => {
                let idx = self.items.len();
                self.items.push(CanonItem::Jet(*j));
                idx
            }
            // Word: record only the bit-width; value will be zeroed at encode time
            Inner::Word(w) => {
                let idx = self.items.len();
                self.items.push(CanonItem::Word(w.n()));
                idx
            }
        };

        self.node_idx.insert(ptr, idx);
        idx
    }
}

/// Encode the items to bytes using the Simplicity bit format.
fn encode_items<W: io::Write>(items: &[CanonItem], w: &mut BitWriter<W>) -> io::Result<()> {
    encode_natural(items.len(), w)?;
    for (i, item) in items.iter().enumerate() {
        match item {
            CanonItem::Comp(l, r) => {
                w.write_bits_be(0b00000, 5)?;
                encode_natural(i - l, w)?;
                encode_natural(i - r, w)?;
            }
            CanonItem::Case(l, r) => {
                w.write_bits_be(0b00001, 5)?;
                encode_natural(i - l, w)?;
                encode_natural(i - r, w)?;
            }
            CanonItem::Pair(l, r) => {
                w.write_bits_be(0b00010, 5)?;
                encode_natural(i - l, w)?;
                encode_natural(i - r, w)?;
            }
            CanonItem::InjL(c) => {
                w.write_bits_be(0b00100, 5)?;
                encode_natural(i - c, w)?;
            }
            CanonItem::InjR(c) => {
                w.write_bits_be(0b00101, 5)?;
                encode_natural(i - c, w)?;
            }
            CanonItem::Take(c) => {
                w.write_bits_be(0b00110, 5)?;
                encode_natural(i - c, w)?;
            }
            CanonItem::Drop(c) => {
                w.write_bits_be(0b00111, 5)?;
                encode_natural(i - c, w)?;
            }
            CanonItem::Disconnect(c) => {
                w.write_bits_be(0b01011, 5)?;
                encode_natural(i - c, w)?;
            }
            CanonItem::Hidden => {
                w.write_bits_be(0b0110, 4)?;
                encode_hash(&[0u8; 32], w)?;
            }
            CanonItem::Iden => {
                w.write_bits_be(0b01000, 5)?;
            }
            CanonItem::Unit => {
                w.write_bits_be(0b01001, 5)?;
            }
            CanonItem::Fail(entropy) => {
                w.write_bits_be(0b01010, 5)?;
                encode_hash(entropy.as_ref(), w)?;
            }
            CanonItem::Witness => {
                w.write_bits_be(0b0111, 4)?;
            }
            CanonItem::Jet(j) => {
                w.write_bit(true)?;
                w.write_bit(true)?;
                j.encode(w)?;
            }
            CanonItem::Word(n) => {
                w.write_bit(true)?;
                w.write_bit(false)?;
                encode_natural(1 + *n as usize, w)?;
                encode_value(zero_word(*n).as_value(), w)?;
            }
        }
    }
    Ok(())
}

/// Produce the canonical program byte encoding of `root`.
fn canonical_encode_to_vec(root: &CommitNode<Elements>) -> Vec<u8> {
    let mut encoder = CanonEncoder::new();
    encoder.visit(root);

    let mut bytes = Vec::new();
    let mut w = BitWriter::new(&mut bytes);
    encode_items(&encoder.items, &mut w).expect("writing to Vec never fails");
    w.flush_all().expect("flushing Vec never fails");
    bytes
}

// ---------------------------------------------------------------------------
// Word helpers
// ---------------------------------------------------------------------------

/// Create an all-zeros [`Word`] of type `2^(2^n)`.
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
