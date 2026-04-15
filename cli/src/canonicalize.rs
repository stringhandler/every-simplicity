use anyhow::{Context, Result};
use simplicity::{
    dag::{DagLike, MaxSharing},
    jet::Elements,
    node::{Commit, CommitNode, Inner},
    BitIter, Cmr, Word,
};

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
