//! `aios-verification` — typed core skeleton for S2.4 Verification Grammar.
//!
//! T-064 intentionally stops at the S2.4 type surface: verification intents,
//! result records, closed status / primitive vocabularies, and the error
//! taxonomy. T-068 adds real composition execution while the gRPC surface,
//! evidence emission, and runtime / renderer integrations remain later M8 tasks.

#![forbid(unsafe_code)]

pub mod engine;
pub mod error;
pub mod executor;
pub mod grammar;
pub mod grammar_parser;
pub mod in_memory_engine;
pub mod intent;
pub mod primitive;
pub mod primitives;
pub mod result;

pub use engine::{VerificationContext, VerificationEngine};
pub use error::VerificationError;
pub use executor::VerificationExecutor;
pub use grammar::{
    PrimitiveInvocation, VerificationDuration, VerificationDurationUnit, VerificationGrammar,
};
pub use in_memory_engine::{compile_intent, InMemoryVerificationEngine};
pub use intent::{IntentId, VerificationIntent};
pub use primitive::VerificationPrimitive;
pub use primitives::{LocalProbe, MockLocalProbe, StdLocalProbe};
pub use result::{PrimitiveResult, VerificationResult, VerificationStatus};
