//! `aios-verification` — typed core skeleton for S2.4 Verification Grammar.
//!
//! T-064 intentionally stops at the S2.4 type surface: verification intents,
//! result records, closed status / primitive vocabularies, and the error
//! taxonomy. The engine trait, primitive implementations, expression parser,
//! executor, gRPC surface, evidence emission, and runtime / renderer
//! integrations land in later M8 tasks.

#![forbid(unsafe_code)]

pub mod error;
pub mod intent;
pub mod primitive;
pub mod result;

pub use error::VerificationError;
pub use intent::{IntentId, VerificationIntent};
pub use primitive::VerificationPrimitive;
pub use result::{PrimitiveResult, VerificationResult, VerificationStatus};
