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
//! The eight ID types in this module cover the S0.1 §3.2.1 registry rows owned by
//! S0.1 itself; later sub-specs (S5.3, S5.4, S10.1, S15.2) add their own newtypes
//! in their respective crates following the same pattern.

use std::fmt;

use serde::{Deserialize, Serialize};
use ulid::Ulid;

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
}
