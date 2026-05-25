//! `aios-verification` — typed core skeleton for S2.4 Verification Grammar.
//!
//! T-064 intentionally stops at the S2.4 type surface: verification intents,
//! result records, closed status / primitive vocabularies, and the error
//! taxonomy. T-066 adds the first real primitive execution tiers while the full
//! expression parser, executor, gRPC surface, evidence emission, and runtime /
//! renderer integrations remain later M8 tasks.

#![forbid(unsafe_code)]

pub mod engine;
pub mod error;
pub mod in_memory_engine;
pub mod intent;
pub mod primitive;
pub mod primitives;
pub mod result;

pub use engine::{VerificationContext, VerificationEngine};
pub use error::VerificationError;
pub use in_memory_engine::InMemoryVerificationEngine;
pub use intent::{IntentId, VerificationIntent};
pub use primitive::VerificationPrimitive;
pub use primitives::{LocalProbe, MockLocalProbe, StdLocalProbe};
pub use result::{PrimitiveResult, VerificationResult, VerificationStatus};
