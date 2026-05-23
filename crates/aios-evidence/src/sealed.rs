//! Type-level encoding of the append-only invariant (INV-005).
//!
//! This module documents — and lightly enforces via a marker trait — the
//! constitutional rule that [`crate::receipt::EvidenceReceipt`] values are
//! immutable after seal.
//!
//! ## How the invariant is enforced
//!
//! 1. **Private fields.** `EvidenceReceipt` has no public field. Direct
//!    mutation requires `&mut field` access; safe Rust cannot reach private
//!    fields from another module.
//! 2. **No `&mut self` accessors.** Every method on `EvidenceReceipt` takes
//!    `&self`. The only place a receipt's internals are written is inside
//!    [`crate::receipt::ReceiptBuilder::seal`], which consumes the builder by
//!    value and returns an owned `EvidenceReceipt`.
//! 3. **Chain API is append-only.** `ReceiptChain::append` is the only public
//!    mutator; there is no `replace`, `remove`, `clear`, or `IndexMut` exposed.
//! 4. **Deserialization re-enters the chain through `append`,** which
//!    re-verifies the link hash before admitting the receipt.
//!
//! ## The [`Sealed`] marker trait
//!
//! `Sealed` is implemented for `EvidenceReceipt` and nothing else. The trait is
//! private to this module so downstream crates cannot implement it for their
//! own types. Generic code that wants to require "this is a sealed evidence
//! receipt" can bound on `Sealed`; this prevents accidental substitution by an
//! unsealed type at compile time.

use crate::receipt::EvidenceReceipt;

mod private {
    /// Private super-trait. External crates cannot name this trait and
    /// therefore cannot implement [`super::Sealed`].
    pub trait SealedSuper {}
}

/// Marker trait identifying a sealed-and-immutable evidence value.
///
/// Implemented only for [`EvidenceReceipt`] in this crate. The bound on
/// [`private::SealedSuper`] makes it impossible for any downstream crate to
/// satisfy this trait, encoding INV-005 at the type level.
pub trait Sealed: private::SealedSuper {}

impl private::SealedSuper for EvidenceReceipt {}
impl Sealed for EvidenceReceipt {}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use crate::receipt::ReceiptBuilder;
    use crate::record::{RecordType, RetentionClass};

    /// Compile-time witness: this generic function only compiles if `T: Sealed`.
    /// Calling it with `EvidenceReceipt` must work; the negative side (calling
    /// with anything else) is enforced by the orphan/coherence rules and is
    /// therefore not expressible as a runtime test.
    fn require_sealed<T: Sealed>(_: &T) {}

    #[test]
    fn evidence_receipt_implements_sealed() {
        let r = ReceiptBuilder::new(
            RecordType::ActionReceived,
            RetentionClass::Standard24M,
            "human:operator-1",
        )
        .seal(None)
        .expect("seal");
        require_sealed(&r);
    }
}
