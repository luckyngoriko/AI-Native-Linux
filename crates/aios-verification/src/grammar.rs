//! Typed AST for the S2.4 verification expression grammar.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S2.4 verification grammar vocabulary"
)]

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::VerificationPrimitive;

/// Parsed S2.4 verification expression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationGrammar {
    /// A single closed-vocabulary primitive invocation.
    Primitive(PrimitiveInvocation),
    /// `all[...]` composition; requires at least two terms in parser input.
    All(Vec<Self>),
    /// `any[...]` composition; requires at least two terms in parser input.
    Any(Vec<Self>),
    /// `not(...)` composition over one term.
    Not(Box<Self>),
    /// `eventually(...)` composition over one term and explicit retry budget.
    Eventually {
        /// Inner expression to retry.
        term: Box<Self>,
        /// Maximum retry duration.
        max_duration: VerificationDuration,
        /// Interval between retries.
        interval: VerificationDuration,
    },
}

/// Closed primitive call plus its per-primitive payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimitiveInvocation {
    /// Primitive kind from the closed S2.4 vocabulary.
    pub kind: VerificationPrimitive,
    /// JSON object containing typed primitive args.
    pub args: Value,
}

/// Duration literal accepted by S2.4 composition grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationDuration {
    /// Non-negative integer duration value.
    pub value: u64,
    /// Unit suffix from the grammar.
    pub unit: VerificationDurationUnit,
}

/// Unit suffix for [`VerificationDuration`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationDurationUnit {
    /// Milliseconds (`ms`).
    Milliseconds,
    /// Seconds (`s`).
    Seconds,
    /// Minutes (`m`).
    Minutes,
    /// Hours (`h`).
    Hours,
}

impl fmt::Display for VerificationGrammar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Primitive(invocation) => invocation.fmt(f),
            Self::All(terms) => write_terms(f, "all", terms),
            Self::Any(terms) => write_terms(f, "any", terms),
            Self::Not(term) => write!(f, "not({term})"),
            Self::Eventually {
                term,
                max_duration,
                interval,
            } => write!(
                f,
                "eventually({term}, max_duration={max_duration}, interval={interval})"
            ),
        }
    }
}

impl fmt::Display for PrimitiveInvocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&primitive_display_name(self.kind))?;
        f.write_str("(")?;
        if let Value::Object(args) = &self.args {
            for (index, (key, value)) in args.iter().enumerate() {
                if index > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "{key}=")?;
                write_json_value(f, value)?;
            }
        }
        f.write_str(")")
    }
}

impl fmt::Display for VerificationDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.value, self.unit)
    }
}

impl fmt::Display for VerificationDurationUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let suffix = match self {
            Self::Milliseconds => "ms",
            Self::Seconds => "s",
            Self::Minutes => "m",
            Self::Hours => "h",
        };
        f.write_str(suffix)
    }
}

fn write_terms(
    f: &mut fmt::Formatter<'_>,
    combinator: &str,
    terms: &[VerificationGrammar],
) -> fmt::Result {
    write!(f, "{combinator}[")?;
    for (index, term) in terms.iter().enumerate() {
        if index > 0 {
            f.write_str(", ")?;
        }
        write!(f, "{term}")?;
    }
    f.write_str("]")
}

fn write_json_value(f: &mut fmt::Formatter<'_>, value: &Value) -> fmt::Result {
    let rendered = serde_json::to_string(value).map_err(|_err| fmt::Error)?;
    f.write_str(&rendered)
}

fn primitive_display_name(kind: VerificationPrimitive) -> String {
    kind.as_wire_str().to_ascii_lowercase()
}
