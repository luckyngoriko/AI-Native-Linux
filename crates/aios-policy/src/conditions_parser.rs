//! Hand-written recursive-descent parser for the §9.1 conditions DSL (T-019).
//!
//! ## Why hand-written
//!
//! The §9.1 grammar is small (a conjunction of predicates with five predicate
//! shapes); a parser combinator crate would add a non-trivial dependency
//! (`pest` / `nom` / `combine`) for a grammar that comfortably fits in ~400 lines
//! of straight Rust. Per §21 stack lock we avoid speculative crate additions.
//!
//! ## Determinism contract
//!
//! Per §13.1 the same source string MUST produce the same AST on every parse, in
//! every process, on every architecture. The parser holds no global mutable state,
//! does no I/O, allocates only owned `String`s in its output, and visits the input
//! exclusively left-to-right. The associated `parse_deterministic_round_trip` test
//! pins this contract by parsing the same source 100 times and asserting AST
//! equality.
//!
//! ## Error reporting
//!
//! Every error carries the 1-based line and column at which it was raised, plus a
//! human-readable English explanation of what was expected vs what was found. The
//! position is computed from the byte offset into the source by walking the input
//! once at error time (the lexer does not carry per-token positions to keep the
//! happy path tight).

use thiserror::Error;

use crate::conditions::{ClosedField, CompareOp, Condition, Namespace, Predicate, Value};

/// Parse a §9.1 condition source string into a typed [`Condition`] AST.
///
/// # Errors
///
/// Returns [`ConditionParseError`] for every parse-time violation: an unknown
/// namespace, an unknown closed field, an unknown operator, an unbalanced bracket,
/// or use of any disallowed grammar token (`or`, `not`, `(`, `)`).
pub fn parse(source: &str) -> Result<Condition, ConditionParseError> {
    let mut parser = Parser::new(source);
    let condition = parser.parse_condition()?;
    parser.expect_eof()?;
    Ok(condition)
}

/// Parser-time failure with a position-accurate report.
///
/// The variants exist so callers can branch on the failure mode for telemetry;
/// the user-facing rendering through [`std::fmt::Display`] is the canonical UX.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConditionParseError {
    /// Token expected but the input ended first.
    #[error("unexpected end of input at line {line}, column {column}: expected {expected}")]
    UnexpectedEof {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// Human description of what the parser was looking for.
        expected: String,
    },
    /// Wrong token encountered.
    #[error(
        "unexpected token at line {line}, column {column}: expected {expected}, found {found}"
    )]
    UnexpectedToken {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// Human description of what was expected.
        expected: String,
        /// Verbatim slice of what was found (up to a short cap).
        found: String,
    },
    /// `namespace` token did not match any of the six closed namespaces.
    #[error("unknown namespace at line {line}, column {column}: {namespace} (closed set: subject, request, target, object, time, system)")]
    UnknownNamespace {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// The offending namespace identifier.
        namespace: String,
    },
    /// `namespace.subpath` did not resolve to a registered closed field.
    #[error("unknown field at line {line}, column {column}: {field} is not in the §9.2 / §26 / §27 / §28 / §29 closed vocabulary")]
    UnknownField {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// The dotted form, e.g. `"subject.spoofed"`.
        field: String,
    },
    /// An operator token outside §9.1's closed set.
    #[error(
        "unknown operator at line {line}, column {column}: {operator} (allowed: =, !=, <, <=, >, >=, in, contains, exists)"
    )]
    UnknownOperator {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// The offending operator token.
        operator: String,
    },
    /// `or`, `not`, `(`, `)` — disallowed by §9.1.
    #[error(
        "disallowed grammar token at line {line}, column {column}: {token} — §9.1 forbids `or`, `not`, and parentheses"
    )]
    DisallowedToken {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// The offending token.
        token: String,
    },
    /// A string literal opened with `"` but never closed.
    #[error("unterminated string literal at line {line}, column {column}")]
    UnterminatedString {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
    },
    /// An invalid integer literal (e.g. `12abc`).
    #[error("invalid integer literal at line {line}, column {column}: {literal}")]
    InvalidInteger {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// The offending literal.
        literal: String,
    },
    /// `IN []` — empty value list.
    #[error("empty value list at line {line}, column {column}: `in` requires at least one value")]
    EmptyValueList {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
    },
}

/// Internal parser state.
struct Parser<'a> {
    source: &'a str,
    /// Byte offset into `source`. The parser only ever increases this.
    pos: usize,
}

impl<'a> Parser<'a> {
    const fn new(source: &'a str) -> Self {
        Self { source, pos: 0 }
    }

    /// Convert the current byte offset to (line, column) for error reporting.
    /// 1-based; column counts UTF-8 chars on the current line.
    fn position(&self) -> (usize, usize) {
        position_for(self.source, self.pos)
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.source.len() {
            let rest = &self.source[self.pos..];
            let mut chars = rest.chars();
            match chars.next() {
                Some(c) if c.is_whitespace() => {
                    self.pos += c.len_utf8();
                }
                _ => break,
            }
        }
    }

    fn at_eof(&mut self) -> bool {
        self.skip_whitespace();
        self.pos >= self.source.len()
    }

    fn rest(&self) -> &'a str {
        &self.source[self.pos..]
    }

    /// Try to consume an exact text token (case-sensitive) after skipping whitespace.
    /// Returns `true` on success and advances the cursor; `false` means no match
    /// and the cursor is unmoved.
    fn try_consume(&mut self, token: &str) -> bool {
        self.skip_whitespace();
        if self.rest().starts_with(token) {
            // ensure the token does not run into an identifier char (so we don't
            // match "in" inside "internal") — only relevant for alphabetic tokens.
            let after_pos = self.pos + token.len();
            if token
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            {
                let next_char = self.source[after_pos..].chars().next();
                if let Some(c) = next_char {
                    if is_ident_continue(c) {
                        return false;
                    }
                }
            }
            self.pos = after_pos;
            true
        } else {
            false
        }
    }

    fn expect_eof(&mut self) -> Result<(), ConditionParseError> {
        if self.at_eof() {
            Ok(())
        } else {
            let (line, column) = self.position();
            Err(ConditionParseError::UnexpectedToken {
                line,
                column,
                expected: "end of input".to_owned(),
                found: short_snippet(self.rest()),
            })
        }
    }

    fn parse_condition(&mut self) -> Result<Condition, ConditionParseError> {
        let mut predicates = Vec::new();
        let first = self.parse_predicate()?;
        predicates.push(first);
        while !self.at_eof() {
            self.reject_disallowed_conjunctions()?;
            // `and` is the only allowed conjunction per §9.1.
            if !self.try_consume("and") {
                let (line, column) = self.position();
                return Err(ConditionParseError::UnexpectedToken {
                    line,
                    column,
                    expected: "`and` or end of input".to_owned(),
                    found: short_snippet(self.rest()),
                });
            }
            let next = self.parse_predicate()?;
            predicates.push(next);
        }
        Ok(Condition::conjunction(predicates))
    }

    fn reject_disallowed_conjunctions(&mut self) -> Result<(), ConditionParseError> {
        self.skip_whitespace();
        for forbidden in ["or", "not"] {
            if self.rest().starts_with(forbidden) {
                let after_pos = self.pos + forbidden.len();
                let next_is_ident = self.source[after_pos..]
                    .chars()
                    .next()
                    .is_some_and(is_ident_continue);
                if !next_is_ident {
                    let (line, column) = self.position();
                    return Err(ConditionParseError::DisallowedToken {
                        line,
                        column,
                        token: forbidden.to_owned(),
                    });
                }
            }
        }
        if self.rest().starts_with('(') || self.rest().starts_with(')') {
            let (line, column) = self.position();
            let token = self
                .rest()
                .chars()
                .next()
                .map_or_else(String::new, |c| c.to_string());
            return Err(ConditionParseError::DisallowedToken {
                line,
                column,
                token,
            });
        }
        Ok(())
    }

    fn parse_predicate(&mut self) -> Result<Predicate, ConditionParseError> {
        self.skip_whitespace();
        self.reject_disallowed_conjunctions()?;

        let field_start = self.pos;
        let field = self.parse_field_path(field_start)?;

        self.skip_whitespace();

        // Operator dispatch: try the multi-char comparison ops first so `<=` is not
        // mis-tokenised as `<`. Then keyword operators (`in`, `contains`, `exists`).
        if let Some(op) = self.try_parse_compare_op() {
            let rhs = self.parse_value()?;
            return Ok(Predicate::Compare { field, op, rhs });
        }

        if self.try_consume("in") {
            let values = self.parse_value_list()?;
            return Ok(Predicate::In { field, values });
        }

        if self.try_consume("contains") {
            let needle = self.parse_string_literal()?;
            return Ok(Predicate::Contains { field, needle });
        }

        if self.try_consume("exists") {
            return Ok(Predicate::Exists { field });
        }

        // Bare `time.recovery_mode` boolean predicate per §9.1 last alternative.
        if matches!(field, ClosedField::TimeRecoveryMode) && self.at_alt_boundary() {
            return Ok(Predicate::Compare {
                field,
                op: CompareOp::Eq,
                rhs: Value::Bool(true),
            });
        }

        let (line, column) = self.position();
        Err(ConditionParseError::UnknownOperator {
            line,
            column,
            operator: short_snippet(self.rest()),
        })
    }

    /// True iff the next non-whitespace position is `and` (or EOF) — used to know
    /// whether the bare `time.recovery_mode` boolean sugar fires or whether an
    /// operator MUST follow.
    fn at_alt_boundary(&mut self) -> bool {
        let saved = self.pos;
        self.skip_whitespace();
        let alt = self.at_eof() || self.rest().starts_with("and");
        self.pos = saved;
        alt
    }

    fn parse_field_path(
        &mut self,
        position_for_errors: usize,
    ) -> Result<ClosedField, ConditionParseError> {
        let namespace_ident = self.parse_identifier()?;
        let namespace = Namespace::from_token(&namespace_ident).ok_or_else(|| {
            let (line, column) = position_for(self.source, position_for_errors);
            ConditionParseError::UnknownNamespace {
                line,
                column,
                namespace: namespace_ident.clone(),
            }
        })?;
        self.skip_whitespace();
        if !self.try_consume_char('.') {
            let (line, column) = self.position();
            return Err(ConditionParseError::UnexpectedToken {
                line,
                column,
                expected: "`.` after namespace".to_owned(),
                found: short_snippet(self.rest()),
            });
        }

        // sub-path may itself contain dots (e.g. `risk.destructive`). Collect every
        // identifier joined by `.` until we hit a token that is NOT a dot-separated
        // identifier continuation (i.e. an operator, `in`, `contains`, `exists`,
        // whitespace, or EOF).
        let mut subpath = self.parse_identifier()?;
        loop {
            let saved = self.pos;
            self.skip_whitespace();
            // peek for another '.' followed immediately by an identifier char.
            if !self.rest().starts_with('.') {
                self.pos = saved;
                break;
            }
            let next_char = self.source[self.pos + 1..].chars().next();
            if !next_char.is_some_and(is_ident_start) {
                self.pos = saved;
                break;
            }
            self.pos += 1; // consume '.'
            let segment = self.parse_identifier()?;
            subpath.push('.');
            subpath.push_str(&segment);
        }

        ClosedField::resolve(namespace, &subpath).ok_or_else(|| {
            let (line, column) = position_for(self.source, position_for_errors);
            ConditionParseError::UnknownField {
                line,
                column,
                field: format!("{}.{}", namespace.as_str(), subpath),
            }
        })
    }

    fn try_parse_compare_op(&mut self) -> Option<CompareOp> {
        // Two-char ops first.
        if self.try_consume_char_seq("!=") {
            return Some(CompareOp::Neq);
        }
        if self.try_consume_char_seq("<=") {
            return Some(CompareOp::Lte);
        }
        if self.try_consume_char_seq(">=") {
            return Some(CompareOp::Gte);
        }
        // Single-char ops.
        if self.try_consume_char('=') {
            return Some(CompareOp::Eq);
        }
        if self.try_consume_char('<') {
            return Some(CompareOp::Lt);
        }
        if self.try_consume_char('>') {
            return Some(CompareOp::Gt);
        }
        None
    }

    fn try_consume_char_seq(&mut self, seq: &str) -> bool {
        self.skip_whitespace();
        if self.rest().starts_with(seq) {
            self.pos += seq.len();
            true
        } else {
            false
        }
    }

    fn try_consume_char(&mut self, c: char) -> bool {
        self.skip_whitespace();
        if self.rest().starts_with(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    fn parse_identifier(&mut self) -> Result<String, ConditionParseError> {
        self.skip_whitespace();
        let start = self.pos;
        let mut iter = self.rest().chars();
        let first = iter.next();
        match first {
            Some(c) if is_ident_start(c) => {
                self.pos += c.len_utf8();
            }
            Some(_) | None => {
                let (line, column) = self.position();
                return Err(ConditionParseError::UnexpectedToken {
                    line,
                    column,
                    expected: "identifier".to_owned(),
                    found: short_snippet(self.rest()),
                });
            }
        }
        while let Some(c) = self.rest().chars().next() {
            if is_ident_continue(c) {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
        Ok(self.source[start..self.pos].to_owned())
    }

    fn parse_value_list(&mut self) -> Result<Vec<Value>, ConditionParseError> {
        self.skip_whitespace();
        if !self.try_consume_char('[') {
            let (line, column) = self.position();
            return Err(ConditionParseError::UnexpectedToken {
                line,
                column,
                expected: "`[` to open value list".to_owned(),
                found: short_snippet(self.rest()),
            });
        }
        let mut values = Vec::new();
        self.skip_whitespace();
        if self.try_consume_char(']') {
            let (line, column) = self.position();
            return Err(ConditionParseError::EmptyValueList { line, column });
        }
        loop {
            let v = self.parse_value()?;
            values.push(v);
            self.skip_whitespace();
            if self.try_consume_char(',') {
                continue;
            }
            if self.try_consume_char(']') {
                break;
            }
            let (line, column) = self.position();
            return Err(ConditionParseError::UnexpectedToken {
                line,
                column,
                expected: "`,` or `]`".to_owned(),
                found: short_snippet(self.rest()),
            });
        }
        Ok(values)
    }

    fn parse_value(&mut self) -> Result<Value, ConditionParseError> {
        self.skip_whitespace();
        let next = self.rest().chars().next();
        match next {
            Some('"') => {
                let s = self.parse_string_literal()?;
                Ok(Value::String(s))
            }
            Some('-' | '0'..='9') => self.parse_number_literal(),
            Some(c) if is_ident_start(c) => {
                let ident = self.parse_identifier()?;
                match ident.as_str() {
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    _ => {
                        // Either an RFC 3339 timestamp (starts with 4-digit year)
                        // or an enum identifier. We never reach this branch for a
                        // pure-digit start because the leading-digit case is handled
                        // above; here `ident` always starts with a letter or `_`,
                        // so treat it as Identifier.
                        Ok(Value::Identifier(ident))
                    }
                }
            }
            Some(_) | None => {
                let (line, column) = self.position();
                Err(ConditionParseError::UnexpectedToken {
                    line,
                    column,
                    expected: "value literal (string, number, bool, identifier, or timestamp)"
                        .to_owned(),
                    found: short_snippet(self.rest()),
                })
            }
        }
    }

    fn parse_number_literal(&mut self) -> Result<Value, ConditionParseError> {
        self.skip_whitespace();
        let start = self.pos;
        if self.rest().starts_with('-') {
            self.pos += 1;
        }
        let digits_start = self.pos;
        while let Some(c) = self.rest().chars().next() {
            if c.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == digits_start {
            let (line, column) = position_for(self.source, start);
            return Err(ConditionParseError::InvalidInteger {
                line,
                column,
                literal: self.source[start..self.pos].to_owned(),
            });
        }
        // Reject fractional / scientific suffixes — §9.1 only models integers.
        if self.rest().starts_with('.')
            && self.source[self.pos + 1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
        {
            // Consume the fractional part to give a precise error literal.
            self.pos += 1;
            while let Some(c) = self.rest().chars().next() {
                if c.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            let (line, column) = position_for(self.source, start);
            return Err(ConditionParseError::InvalidInteger {
                line,
                column,
                literal: self.source[start..self.pos].to_owned(),
            });
        }
        let lit = &self.source[start..self.pos];
        let parsed = lit.parse::<i64>().map_err(|_| {
            let (line, column) = position_for(self.source, start);
            ConditionParseError::InvalidInteger {
                line,
                column,
                literal: lit.to_owned(),
            }
        })?;
        Ok(Value::Int(parsed))
    }

    fn parse_string_literal(&mut self) -> Result<String, ConditionParseError> {
        self.skip_whitespace();
        let open = self.position();
        if !self.try_consume_char('"') {
            return Err(ConditionParseError::UnexpectedToken {
                line: open.0,
                column: open.1,
                expected: "`\"`-quoted string literal".to_owned(),
                found: short_snippet(self.rest()),
            });
        }
        let start = self.pos;
        loop {
            let next = self.rest().chars().next();
            match next {
                Some('"') => {
                    let s = self.source[start..self.pos].to_owned();
                    self.pos += 1; // consume closing quote
                    return Ok(s);
                }
                Some('\\') => {
                    // Support `\"` and `\\` escapes; everything else passes through verbatim.
                    self.pos += 1;
                    if let Some(esc) = self.rest().chars().next() {
                        self.pos += esc.len_utf8();
                    } else {
                        return Err(ConditionParseError::UnterminatedString {
                            line: open.0,
                            column: open.1,
                        });
                    }
                }
                Some(c) => {
                    self.pos += c.len_utf8();
                }
                None => {
                    return Err(ConditionParseError::UnterminatedString {
                        line: open.0,
                        column: open.1,
                    });
                }
            }
        }
    }
}

const fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

const fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Convert a byte offset into (line, column), 1-based.
fn position_for(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1_usize;
    let mut col = 1_usize;
    let clamped = offset.min(source.len());
    for c in source[..clamped].chars() {
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Short, single-line excerpt of the remaining input for error messages.
fn short_snippet(rest: &str) -> String {
    let trimmed = rest.trim_end();
    let line = trimmed.split('\n').next().unwrap_or("");
    if line.chars().count() > 32 {
        let head: String = line.chars().take(32).collect();
        format!("{head}…")
    } else if line.is_empty() {
        "<end of input>".to_owned()
    } else {
        line.to_owned()
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    #[test]
    fn position_for_handles_multi_line_offsets() {
        let source = "a\nbc\nd";
        assert_eq!(position_for(source, 0), (1, 1));
        assert_eq!(position_for(source, 1), (1, 2));
        assert_eq!(position_for(source, 2), (2, 1));
        assert_eq!(position_for(source, 3), (2, 2));
        assert_eq!(position_for(source, 5), (3, 1));
    }

    #[test]
    fn position_for_clamps_to_eof() {
        let source = "abc";
        let pos = position_for(source, 99);
        assert_eq!(pos, (1, 4));
    }

    #[test]
    fn parse_single_eq_predicate() {
        let c = parse("subject.recovery_mode = false").expect("parses");
        assert_eq!(c.predicates.len(), 1);
        match &c.predicates[0] {
            Predicate::Compare { field, op, rhs } => {
                assert_eq!(*field, ClosedField::SubjectRecoveryMode);
                assert_eq!(*op, CompareOp::Eq);
                assert_eq!(*rhs, Value::Bool(false));
            }
            other => panic!("expected Compare, got {other:?}"),
        }
    }

    #[test]
    fn parse_short_snippet_caps_long_lines() {
        let long = "a".repeat(64);
        let snippet = short_snippet(&long);
        assert!(snippet.ends_with('…'));
    }
}
