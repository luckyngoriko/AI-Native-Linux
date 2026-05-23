//! Prefix-namespaced ULID identifiers (S0.1 §3.2).
//!
//! Every system-generated id is a Crockford-base32 ULID prefixed with a fixed namespace
//! tag followed by an **underscore** — e.g. `act_01HXY8K2JPQ7N3M4R5S6T7V8W9`.
//!
//! ## Wave-11 constitutional rule
//!
//! Colon-separated forms (`act:01H...`) are **forbidden** as of S0.1 §3.2 Wave-11
//! normalisation. They are reserved as a sentinel for legacy/illegal input; every
//! parser in this crate MUST reject them with [`IdError::ColonSeparatorForbidden`].
//!
//! This module covers the full S0.1 §3.2.1 prefix-namespace registry:
//!
//! - **ULID-bodied** (13 rows): `act_`, `intent_`, `plan_`, `corr_`, `evr_`,
//!   `polreq_`, `poldec_`, `appr_`, `apprq_`, `appb_`, `ovrq_`, `ovr_`, `actrq_`.
//! - **Content-addressed** (1 row): `tplan_` — body is
//!   `hex_lower(BLAKE3(JCS(canonical_form)))[:32]` per S0.1 §3.2.2 / S15.2 §5.3.
//!
//! All 14 ID types are minted/validated here. Higher-layer sub-specs (S5.3, S5.4,
//! S10.1, S15.2) consume these newtypes rather than redefining the wire format.

use std::fmt;

use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::canonical::blake3_truncated;
use crate::error::IdError;

/// Internal helper: validate `(prefix, body)` shape and reject the forbidden colon form.
///
/// Returns the normalised owned string `"<prefix><body>"` on success.
fn validate_prefixed_ulid(input: &str, expected_prefix: &'static str) -> Result<String, IdError> {
    if input.is_empty() {
        return Err(IdError::Empty);
    }

    // Wave-11 §3.2: the underscore separator is the ONLY accepted form. The colon form
    // is reserved as a sentinel for legacy/illegal input and MUST be rejected here.
    if let Some(stripped) = expected_prefix.strip_suffix('_') {
        let colon_form = format!("{stripped}:");
        if input.starts_with(&colon_form) {
            return Err(IdError::ColonSeparatorForbidden(input.to_owned()));
        }
    }

    let Some(body) = input.strip_prefix(expected_prefix) else {
        return Err(IdError::WrongPrefix {
            expected: expected_prefix,
            got: input.to_owned(),
        });
    };

    Ulid::from_string(body).map_err(|e| IdError::InvalidUlidBody {
        id: input.to_owned(),
        detail: e.to_string(),
    })?;

    Ok(input.to_owned())
}

/// Internal helper: mint a fresh `<prefix><ULID>` string with a current-timestamp ULID.
fn fresh(prefix: &'static str) -> String {
    let body = Ulid::new().to_string();
    format!("{prefix}{body}")
}

/// Generate the eight S0.1-owned ID newtypes from a single declarative macro.
///
/// Each newtype is a tuple struct wrapping a `String` that has been validated to match
/// `<prefix>_<26-char ULID>`. Constructors:
///
/// - [`Self::new`]  — mint a fresh id from a current-timestamp ULID.
/// - [`Self::parse`] — validate and adopt an externally supplied string.
/// - [`Self::as_str`] — borrow the underlying canonical form.
macro_rules! define_prefixed_id {
    ($(#[$meta:meta])* $name:ident, $prefix:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// The canonical prefix including the trailing underscore.
            pub const PREFIX: &'static str = $prefix;

            /// Mint a fresh identifier with a current-timestamp ULID body.
            #[must_use]
            pub fn new() -> Self {
                Self(fresh(Self::PREFIX))
            }

            /// Validate and adopt an externally supplied string.
            ///
            /// # Errors
            ///
            /// Returns [`IdError`] when:
            /// - the input is empty,
            /// - the prefix does not match `PREFIX`,
            /// - the input uses the forbidden colon separator (`{prefix}:01H...`),
            /// - the ULID body fails to parse.
            pub fn parse(input: &str) -> Result<Self, IdError> {
                validate_prefixed_ulid(input, Self::PREFIX).map(Self)
            }

            /// Borrow the canonical `<prefix><ULID>` string.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

define_prefixed_id!(
    /// Canonical action envelope identity — S0.1 §3.2.1 row `act_`.
    ActionId, "act_"
);
define_prefixed_id!(
    /// User intent object — S0.1 §3.2.1 row `intent_`.
    IntentId, "intent_"
);
define_prefixed_id!(
    /// Multi-step plan — S0.1 §3.2.1 row `plan_`.
    PlanId, "plan_"
);
define_prefixed_id!(
    /// Workflow correlation id (broader than `intent_id`) — S0.1 §3.2.1 row `corr_`.
    CorrelationId, "corr_"
);
define_prefixed_id!(
    /// Evidence receipt id — S0.1 §3.2.1 row `evr_` (also referenced by L9 S3.1).
    EvidenceReceiptId, "evr_"
);
define_prefixed_id!(
    /// Policy request id — S0.1 §3.2.1 row `polreq_`.
    PolicyRequestId, "polreq_"
);
define_prefixed_id!(
    /// Policy decision id — S0.1 §3.2.1 row `poldec_`.
    PolicyDecisionId, "poldec_"
);
define_prefixed_id!(
    /// Legacy approval-receipt id — S0.1 §3.2.1 row `appr_` (reserved; concrete approval workflows use `apprq_` / `appb_` from S5.3).
    ApprovalId, "appr_"
);

// ---------------------------------------------------------------------------
// Extended namespaces — T-003 (S5.3, S5.4, S10.1 ULID-bodied rows).
//
// These mirror the eight S0.1-owned rows above: ULID body, underscore separator,
// colon form rejected. Ownership of the semantics lives in the cited sub-spec;
// only the wire-format newtype lives here.
// ---------------------------------------------------------------------------

define_prefixed_id!(
    /// Approval request id — S5.3 §4.
    ///
    /// Short-lived workflow object the Policy Kernel emits when its outcome is
    /// `REQUIRE_APPROVAL`; consumed by the Approval Mechanics FSM. The `apprq_`
    /// namespace is distinct from the `appb_` binding receipt produced on `GRANTED`.
    ApprovalRequestId, "apprq_"
);
define_prefixed_id!(
    /// Approval binding id — S5.3 §5.
    ///
    /// Durable, signed receipt of operator consent that the Capability Runtime
    /// consumes to advance a `policy_pending` action whose policy outcome was
    /// `REQUIRE_APPROVAL`. Single-use for `EXACT_ACTION` scope; never minted
    /// before `GRANTED`.
    ApprovalBindingId, "appb_"
);
define_prefixed_id!(
    /// Override request id — S5.4 §4.
    ///
    /// Artifact authored by the requesting subject when seeking to lift a
    /// hard-deny policy rule under quorum; the id always precedes any `ovr_`
    /// binding for the same logical override.
    OverrideRequestId, "ovrq_"
);
define_prefixed_id!(
    /// Override binding id — S5.4 §5.
    ///
    /// Issued only after quorum and channel separation are satisfied; the
    /// artifact the Capability Runtime consults in place of a hard-deny. The
    /// `ovrq_` / `ovr_` split forecloses audit ambiguity between "asked" and
    /// "granted".
    OverrideBindingId, "ovr_"
);
define_prefixed_id!(
    /// Action runtime request id — S10.1 (Capability Runtime gRPC).
    ///
    /// The L3 per-attempt queue handle for one execution of an action envelope.
    /// Distinct from `act_`: two `actrq_` ids may exist for the same `act_`
    /// envelope across retries.
    ActionRuntimeRequestId, "actrq_"
);

// ---------------------------------------------------------------------------
// Content-addressed namespace — `tplan_` (S15.2 §5.3 + S0.1 §3.2.2).
//
// `TransitionPlanId` is structurally different from the ULID-bodied newtypes:
//
//   transition_plan_id = "tplan_" || hex_lower(BLAKE3(jcs(canonicalized_plan)))[:32]
//
// Properties this enforces:
//
// - Constructor `from_content(bytes)` is deterministic: same input → same id.
// - There is NO `new()` random constructor — content addressing is the whole
//   point of the namespace; allowing fresh-random ids would silently violate
//   the byte-identical-replay invariant of S15.2 §5.3 (item 5).
// - `parse` accepts exactly `tplan_` + 32 lowercase hex chars; everything else
//   is an error.
// ---------------------------------------------------------------------------

/// Transition plan id — S15.2 §5.3. Content-addressed identifier whose body is
/// `hex_lower(BLAKE3(jcs(canonicalized_plan)))[:32]` per S0.1 §3.2.2.
///
/// Unlike the ULID-bodied newtypes in this module, `TransitionPlanId` does NOT
/// expose a random `new()` constructor: identical canonical plans MUST produce
/// byte-identical ids (S15.2 §5.3 invariant). Use [`Self::from_content`] to
/// derive an id from the canonical bytes of the plan, or [`Self::parse`] to
/// validate an externally supplied string.
///
/// ## Wave-11 constitutional rule
///
/// Colon-separated forms (`tplan:abc...`) are forbidden and rejected with
/// [`IdError::ColonSeparatorForbidden`], identical to the ULID-bodied newtypes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TransitionPlanId(String);

impl TransitionPlanId {
    /// The canonical prefix including the trailing underscore.
    pub const PREFIX: &'static str = "tplan_";

    /// Required body length: 32 lowercase hex chars = 128-bit BLAKE3 truncation.
    pub const BODY_LEN: usize = 32;

    /// Derive a deterministic id from arbitrary content bytes.
    ///
    /// The caller is responsible for passing already-canonicalized bytes
    /// (typically the output of [`crate::canonical::jcs_canonicalize`] applied
    /// to a `TransitionPlan` value). This constructor does not canonicalize
    /// for you — that decision lives one layer up so the canonical form
    /// remains explicit at every call site.
    #[must_use]
    pub fn from_content(bytes: &[u8]) -> Self {
        let body = blake3_truncated(bytes);
        Self(format!("{}{body}", Self::PREFIX))
    }

    /// Validate and adopt an externally supplied string.
    ///
    /// # Errors
    ///
    /// Returns [`IdError`] when:
    /// - the input is empty,
    /// - the input uses the forbidden colon separator (`tplan:abc...`),
    /// - the prefix is not `tplan_`,
    /// - the body is not exactly 32 lowercase hex characters.
    pub fn parse(input: &str) -> Result<Self, IdError> {
        if input.is_empty() {
            return Err(IdError::Empty);
        }

        // Wave-11 §3.2: reject the colon-separated form before anything else.
        if input.starts_with("tplan:") {
            return Err(IdError::ColonSeparatorForbidden(input.to_owned()));
        }

        let Some(body) = input.strip_prefix(Self::PREFIX) else {
            return Err(IdError::WrongPrefix {
                expected: Self::PREFIX,
                got: input.to_owned(),
            });
        };

        if body.len() != Self::BODY_LEN {
            return Err(IdError::InvalidHexBody {
                id: input.to_owned(),
                detail: format!(
                    "expected {}-char lowercase hex body, got {} chars",
                    Self::BODY_LEN,
                    body.len(),
                ),
            });
        }

        if !body
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        {
            return Err(IdError::InvalidHexBody {
                id: input.to_owned(),
                detail: "body must be lowercase hex [0-9a-f] only".to_owned(),
            });
        }

        Ok(Self(input.to_owned()))
    }

    /// Borrow the canonical `tplan_<32hex>` string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TransitionPlanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for TransitionPlanId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    #[test]
    fn action_id_roundtrip_via_parse() {
        let minted = ActionId::new();
        let raw = minted.as_str().to_owned();

        // Shape: "act_" + 26-char Crockford base32 ULID.
        assert!(raw.starts_with("act_"), "expected `act_` prefix, got {raw}");
        assert_eq!(raw.len(), "act_".len() + 26, "ULID body must be 26 chars");

        let reparsed = ActionId::parse(&raw).expect("freshly minted id must reparse");
        assert_eq!(reparsed, minted, "round-trip must preserve the value");
    }

    #[test]
    fn action_id_rejects_colon_separator_wave_11_rule() {
        // S0.1 §3.2 Wave-11: colon-separated forms are a sentinel for legacy/illegal input
        // and MUST be rejected. We test against a syntactically valid ULID body to prove
        // that it's the separator alone that triggers rejection.
        let body = Ulid::new().to_string();
        let colon_form = format!("act:{body}");

        let err = ActionId::parse(&colon_form).expect_err("colon form MUST be rejected");
        assert!(
            matches!(&err, IdError::ColonSeparatorForbidden(s) if s == &colon_form),
            "expected ColonSeparatorForbidden, got {err:?}",
        );
    }

    #[test]
    fn parse_rejects_wrong_prefix() {
        let intent = IntentId::new();
        let err = ActionId::parse(intent.as_str()).expect_err("must reject mismatched prefix");
        assert!(matches!(
            err,
            IdError::WrongPrefix {
                expected: "act_",
                ..
            }
        ));
    }

    #[test]
    fn parse_rejects_empty_and_malformed_body() {
        assert!(matches!(ActionId::parse(""), Err(IdError::Empty)));
        let err = ActionId::parse("act_not-a-real-ulid").expect_err("malformed body must fail");
        assert!(matches!(err, IdError::InvalidUlidBody { .. }));
    }

    #[test]
    fn all_eight_namespaces_round_trip() {
        // Belt-and-braces: every S0.1-owned namespace must satisfy `parse(new()) == new()`.
        assert_eq!(
            ActionId::parse(ActionId::new().as_str()).map(|v| v.as_str().len()),
            Ok("act_".len() + 26)
        );
        assert_eq!(
            IntentId::parse(IntentId::new().as_str()).map(|v| v.as_str().len()),
            Ok("intent_".len() + 26)
        );
        assert_eq!(
            PlanId::parse(PlanId::new().as_str()).map(|v| v.as_str().len()),
            Ok("plan_".len() + 26)
        );
        assert_eq!(
            CorrelationId::parse(CorrelationId::new().as_str()).map(|v| v.as_str().len()),
            Ok("corr_".len() + 26)
        );
        assert_eq!(
            EvidenceReceiptId::parse(EvidenceReceiptId::new().as_str()).map(|v| v.as_str().len()),
            Ok("evr_".len() + 26)
        );
        assert_eq!(
            PolicyRequestId::parse(PolicyRequestId::new().as_str()).map(|v| v.as_str().len()),
            Ok("polreq_".len() + 26)
        );
        assert_eq!(
            PolicyDecisionId::parse(PolicyDecisionId::new().as_str()).map(|v| v.as_str().len()),
            Ok("poldec_".len() + 26)
        );
        assert_eq!(
            ApprovalId::parse(ApprovalId::new().as_str()).map(|v| v.as_str().len()),
            Ok("appr_".len() + 26)
        );
    }

    // -----------------------------------------------------------------------
    // T-003 — extended ULID-bodied namespaces (S5.3 / S5.4 / S10.1).
    // -----------------------------------------------------------------------

    #[test]
    fn extended_ulid_namespaces_round_trip_via_parse() {
        // Belt-and-braces: every ULID-bodied row added in T-003 must satisfy
        // `parse(new()) == new()` and carry the registered prefix.
        let apprq = ApprovalRequestId::new();
        assert!(apprq.as_str().starts_with("apprq_"));
        assert_eq!(apprq.as_str().len(), "apprq_".len() + 26);
        assert_eq!(
            ApprovalRequestId::parse(apprq.as_str()).expect("apprq_ must reparse"),
            apprq
        );

        let appb = ApprovalBindingId::new();
        assert!(appb.as_str().starts_with("appb_"));
        assert_eq!(appb.as_str().len(), "appb_".len() + 26);
        assert_eq!(
            ApprovalBindingId::parse(appb.as_str()).expect("appb_ must reparse"),
            appb
        );

        let ovrq = OverrideRequestId::new();
        assert!(ovrq.as_str().starts_with("ovrq_"));
        assert_eq!(ovrq.as_str().len(), "ovrq_".len() + 26);
        assert_eq!(
            OverrideRequestId::parse(ovrq.as_str()).expect("ovrq_ must reparse"),
            ovrq
        );

        let ovr = OverrideBindingId::new();
        assert!(ovr.as_str().starts_with("ovr_"));
        assert_eq!(ovr.as_str().len(), "ovr_".len() + 26);
        assert_eq!(
            OverrideBindingId::parse(ovr.as_str()).expect("ovr_ must reparse"),
            ovr
        );

        let actrq = ActionRuntimeRequestId::new();
        assert!(actrq.as_str().starts_with("actrq_"));
        assert_eq!(actrq.as_str().len(), "actrq_".len() + 26);
        assert_eq!(
            ActionRuntimeRequestId::parse(actrq.as_str()).expect("actrq_ must reparse"),
            actrq
        );
    }

    #[test]
    fn extended_ulid_namespaces_reject_colon_separator_form() {
        // S0.1 §3.2 Wave-11: every new namespace MUST also reject the colon form.
        let body = Ulid::new().to_string();

        for (input, namespace) in [
            (format!("apprq:{body}"), "apprq"),
            (format!("appb:{body}"), "appb"),
            (format!("ovrq:{body}"), "ovrq"),
            (format!("ovr:{body}"), "ovr"),
            (format!("actrq:{body}"), "actrq"),
        ] {
            let err = match namespace {
                "apprq" => ApprovalRequestId::parse(&input).err(),
                "appb" => ApprovalBindingId::parse(&input).err(),
                "ovrq" => OverrideRequestId::parse(&input).err(),
                "ovr" => OverrideBindingId::parse(&input).err(),
                "actrq" => ActionRuntimeRequestId::parse(&input).err(),
                _ => unreachable!(),
            }
            .expect("colon form MUST be rejected");
            assert!(
                matches!(&err, IdError::ColonSeparatorForbidden(s) if s == &input),
                "{namespace}: expected ColonSeparatorForbidden, got {err:?}",
            );
        }
    }

    #[test]
    fn ovr_does_not_swallow_ovrq_input_via_prefix_collision() {
        // Subtle invariant: `ovr_` is a strict prefix of `ovrq_` as a Rust
        // string, but the registered ULID prefixes are `ovr_` and `ovrq_`
        // (both ending in `_`). Feeding an `ovrq_` id to OverrideBindingId
        // must fail — the ULID body starts with the literal `q`, which is
        // NOT valid Crockford base32, so the ULID parser rejects it.
        let ovrq = OverrideRequestId::new();
        let err =
            OverrideBindingId::parse(ovrq.as_str()).expect_err("ovrq_ must not parse as ovr_");
        // Either of these two errors is acceptable — both prove the prefix
        // collision is caught. In the current implementation the input
        // starts with `ovr_` so the prefix strip succeeds and the ULID
        // parser then rejects the leading `q`.
        assert!(
            matches!(
                err,
                IdError::InvalidUlidBody { .. } | IdError::WrongPrefix { .. }
            ),
            "expected InvalidUlidBody or WrongPrefix, got: {err:?}"
        );
    }

    // -----------------------------------------------------------------------
    // T-003 — content-addressed `tplan_` namespace (S15.2 §5.3 + S0.1 §3.2.2).
    // -----------------------------------------------------------------------

    #[test]
    fn transition_plan_id_from_content_is_deterministic() {
        // Same input → byte-identical id (S15.2 §5.3 invariant 5).
        let a = TransitionPlanId::from_content(b"hello");
        let b = TransitionPlanId::from_content(b"hello");
        assert_eq!(a, b, "same input bytes must yield byte-identical id");
        assert_eq!(a.as_str(), b.as_str());

        // Shape: `tplan_` + 32 hex chars.
        assert!(a.as_str().starts_with("tplan_"));
        assert_eq!(a.as_str().len(), "tplan_".len() + 32);
    }

    #[test]
    fn transition_plan_id_from_different_content_differs() {
        // Distinct logical inputs MUST produce distinct ids (otherwise content
        // addressing is broken and the result cache collides across plans).
        let a = TransitionPlanId::from_content(b"hello");
        let b = TransitionPlanId::from_content(b"world");
        assert_ne!(a, b, "distinct content must yield distinct ids");
    }

    #[test]
    fn transition_plan_id_parse_accepts_valid_form() {
        // Round-trip the derived id through the parser.
        let derived = TransitionPlanId::from_content(b"some-canonical-plan");
        let reparsed = TransitionPlanId::parse(derived.as_str()).expect("derived id must reparse");
        assert_eq!(reparsed, derived);
    }

    #[test]
    fn transition_plan_id_parse_rejects_bad_inputs() {
        // (1) Empty.
        assert!(matches!(TransitionPlanId::parse(""), Err(IdError::Empty)));

        // (2) Wave-11 colon form.
        let colon = "tplan:0123456789abcdef0123456789abcdef";
        let err = TransitionPlanId::parse(colon).expect_err("colon form MUST be rejected");
        assert!(
            matches!(&err, IdError::ColonSeparatorForbidden(s) if s == colon),
            "expected ColonSeparatorForbidden, got {err:?}"
        );

        // (3) Wrong prefix.
        let err = TransitionPlanId::parse("plan_0123456789abcdef0123456789abcdef")
            .expect_err("wrong prefix MUST be rejected");
        assert!(matches!(
            err,
            IdError::WrongPrefix {
                expected: "tplan_",
                ..
            }
        ));

        // (4) Too short — 31 hex chars.
        let too_short = "tplan_0123456789abcdef0123456789abcde";
        let err = TransitionPlanId::parse(too_short).expect_err("31-char body MUST be rejected");
        assert!(
            matches!(&err, IdError::InvalidHexBody { id, .. } if id == too_short),
            "expected InvalidHexBody, got {err:?}"
        );

        // (5) Too long — 33 hex chars.
        let too_long = "tplan_0123456789abcdef0123456789abcdef0";
        let err = TransitionPlanId::parse(too_long).expect_err("33-char body MUST be rejected");
        assert!(
            matches!(&err, IdError::InvalidHexBody { id, .. } if id == too_long),
            "expected InvalidHexBody, got {err:?}"
        );

        // (6) Non-hex character.
        let non_hex = "tplan_0123456789abcdef0123456789abcdeg";
        let err = TransitionPlanId::parse(non_hex).expect_err("non-hex MUST be rejected");
        assert!(
            matches!(&err, IdError::InvalidHexBody { id, .. } if id == non_hex),
            "expected InvalidHexBody, got {err:?}"
        );

        // (7) Uppercase hex (W11-B insists on lowercase).
        let upper = "tplan_0123456789ABCDEF0123456789abcdef";
        let err = TransitionPlanId::parse(upper).expect_err("uppercase MUST be rejected");
        assert!(
            matches!(&err, IdError::InvalidHexBody { id, .. } if id == upper),
            "expected InvalidHexBody for uppercase, got {err:?}"
        );
    }

    #[test]
    fn transition_plan_id_body_matches_blake3_truncated_helper() {
        // The id body MUST be exactly what `blake3_truncated` returns for the
        // same input — this pins us to the W11-B universal truncation rule
        // (S0.1 §3.2.2) and prevents drift if the helper is ever swapped.
        use crate::canonical::blake3_truncated;

        let input = b"plan-content-bytes";
        let id = TransitionPlanId::from_content(input);
        let expected_body = blake3_truncated(input);
        assert_eq!(
            id.as_str(),
            format!("tplan_{expected_body}"),
            "id body must equal blake3_truncated(input)"
        );
    }
}
