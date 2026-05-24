//! Hand-written parser for the AIOS-FS query predicate DSL.

#![allow(
    clippy::module_name_repetitions,
    reason = "AIOS-FS public names mirror the spec vocabulary"
)]

use chrono::DateTime;
use thiserror::Error;

use crate::query::{Predicate, Query, QueryField, QueryNamespace, QueryOperator, QueryValue};

/// Parse query source into the typed [`Query`] AST.
///
/// # Errors
///
/// Returns [`QueryParseError`] when the source is empty, uses a field/operator
/// outside the closed vocabulary, or attempts disallowed grammar (`or`, `not`, or
/// parentheses).
pub fn parse(source: &str) -> Result<Query, QueryParseError> {
    let mut parser = Parser::new(source);
    let query = parser.parse_query()?;
    parser.expect_eof()?;
    Ok(query)
}

/// Parser-time failure with position details.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QueryParseError {
    /// Input ended before the expected token.
    #[error("unexpected end of input at line {line}, column {column}: expected {expected}")]
    UnexpectedEof {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// Expected token description.
        expected: String,
    },
    /// Wrong token at the current position.
    #[error(
        "unexpected token at line {line}, column {column}: expected {expected}, found {found}"
    )]
    UnexpectedToken {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// Expected token description.
        expected: String,
        /// Found token excerpt.
        found: String,
    },
    /// Namespace token is outside the closed namespace vocabulary.
    #[error("unknown namespace at line {line}, column {column}: {namespace} (closed set: object, version, pointer, chunk, namespace)")]
    UnknownNamespace {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// Offending namespace token.
        namespace: String,
    },
    /// Dotted field is outside the closed field vocabulary.
    #[error("unknown field at line {line}, column {column}: {field}")]
    UnknownField {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// Offending dotted field.
        field: String,
    },
    /// Operator token is outside the closed operator vocabulary.
    #[error("unknown operator at line {line}, column {column}: {operator} (allowed: =, !=, <, <=, >, >=, in, contains, matches)")]
    UnknownOperator {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// Offending operator token.
        operator: String,
    },
    /// Explicitly disallowed grammar token.
    #[error("disallowed grammar token at line {line}, column {column}: {token}")]
    DisallowedToken {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// Disallowed token.
        token: String,
    },
    /// String literal was not closed.
    #[error("unterminated string literal at line {line}, column {column}")]
    UnterminatedString {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
    },
    /// Integer literal could not be parsed as `i64`.
    #[error("invalid integer literal at line {line}, column {column}: {literal}")]
    InvalidInteger {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// Offending literal.
        literal: String,
    },
    /// Empty `in []` list.
    #[error("empty value list at line {line}, column {column}: `in` requires values")]
    EmptyValueList {
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
    },
}

struct Parser<'a> {
    source: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    const fn new(source: &'a str) -> Self {
        Self { source, pos: 0 }
    }

    fn parse_query(&mut self) -> Result<Query, QueryParseError> {
        let mut predicates = Vec::new();
        predicates.push(self.parse_predicate()?);

        while !self.at_eof() {
            self.reject_disallowed_tokens()?;
            if !self.try_consume_keyword("and") {
                let (line, column) = self.position();
                return Err(QueryParseError::UnexpectedToken {
                    line,
                    column,
                    expected: "`and` or end of input".to_owned(),
                    found: short_snippet(self.rest()),
                });
            }
            predicates.push(self.parse_predicate()?);
        }

        Ok(Query::And(predicates))
    }

    fn parse_predicate(&mut self) -> Result<Predicate, QueryParseError> {
        self.skip_whitespace();
        self.reject_disallowed_tokens()?;

        let field_start = self.pos;
        let (namespace, field) = self.parse_field_path(field_start)?;

        self.skip_whitespace();
        let (op, rhs) = if let Some(op) = self.try_parse_compare_operator() {
            (op, self.parse_value()?)
        } else if self.try_consume_keyword("in") {
            (QueryOperator::In, self.parse_value_list()?)
        } else if self.try_consume_keyword("contains") {
            (
                QueryOperator::Contains,
                QueryValue::String(self.parse_string_literal()?),
            )
        } else if self.try_consume_keyword("matches") {
            (
                QueryOperator::Matches,
                QueryValue::String(self.parse_string_literal()?),
            )
        } else {
            let (line, column) = self.position();
            return Err(QueryParseError::UnknownOperator {
                line,
                column,
                operator: short_snippet(self.rest()),
            });
        };

        Ok(Predicate {
            namespace,
            field,
            op,
            rhs,
        })
    }

    fn parse_field_path(
        &mut self,
        position_for_errors: usize,
    ) -> Result<(QueryNamespace, QueryField), QueryParseError> {
        let namespace_token = self.parse_identifier()?;
        let namespace = QueryNamespace::from_token(&namespace_token).ok_or_else(|| {
            let (line, column) = position_for(self.source, position_for_errors);
            QueryParseError::UnknownNamespace {
                line,
                column,
                namespace: namespace_token.clone(),
            }
        })?;

        if !self.try_consume_char('.') {
            let (line, column) = self.position();
            return Err(QueryParseError::UnexpectedToken {
                line,
                column,
                expected: "`.` after namespace".to_owned(),
                found: short_snippet(self.rest()),
            });
        }

        let mut subpath = self.parse_identifier()?;
        loop {
            let saved = self.pos;
            self.skip_whitespace();
            if !self.rest().starts_with('.') {
                self.pos = saved;
                break;
            }
            let Some(next) = self.source[self.pos + 1..].chars().next() else {
                self.pos = saved;
                break;
            };
            if !is_ident_start(next) {
                self.pos = saved;
                break;
            }
            self.pos += 1;
            let segment = self.parse_identifier()?;
            subpath.push('.');
            subpath.push_str(&segment);
        }

        let field = QueryField::resolve(namespace, &subpath).ok_or_else(|| {
            let (line, column) = position_for(self.source, position_for_errors);
            QueryParseError::UnknownField {
                line,
                column,
                field: format!("{}.{}", namespace.as_str(), subpath),
            }
        })?;

        Ok((namespace, field))
    }

    fn parse_value(&mut self) -> Result<QueryValue, QueryParseError> {
        self.skip_whitespace();
        self.reject_parentheses()?;

        match self.rest().chars().next() {
            Some('"') => Ok(QueryValue::String(self.parse_string_literal()?)),
            Some('-' | '0'..='9') => self.parse_integer_literal().map(QueryValue::Int),
            Some(c) if is_ident_start(c) => {
                let ident = self.parse_identifier()?;
                match ident.as_str() {
                    "true" => Ok(QueryValue::Bool(true)),
                    "false" => Ok(QueryValue::Bool(false)),
                    _ => Ok(QueryValue::String(ident)),
                }
            }
            Some(_) => {
                let (line, column) = self.position();
                Err(QueryParseError::UnexpectedToken {
                    line,
                    column,
                    expected: "value literal".to_owned(),
                    found: short_snippet(self.rest()),
                })
            }
            None => {
                let (line, column) = self.position();
                Err(QueryParseError::UnexpectedEof {
                    line,
                    column,
                    expected: "value literal".to_owned(),
                })
            }
        }
    }

    fn parse_value_list(&mut self) -> Result<QueryValue, QueryParseError> {
        if !self.try_consume_char('[') {
            let (line, column) = self.position();
            return Err(QueryParseError::UnexpectedToken {
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
            return Err(QueryParseError::EmptyValueList { line, column });
        }

        loop {
            values.push(self.parse_list_string_value()?);
            self.skip_whitespace();
            if self.try_consume_char(',') {
                continue;
            }
            if self.try_consume_char(']') {
                break;
            }

            let (line, column) = self.position();
            return Err(QueryParseError::UnexpectedToken {
                line,
                column,
                expected: "`,` or `]`".to_owned(),
                found: short_snippet(self.rest()),
            });
        }

        if let [start, end] = values.as_slice() {
            if is_rfc3339(start) && is_rfc3339(end) {
                return Ok(QueryValue::TimeRange {
                    start: start.clone(),
                    end: end.clone(),
                });
            }
        }

        Ok(QueryValue::StringList(values))
    }

    fn parse_list_string_value(&mut self) -> Result<String, QueryParseError> {
        self.skip_whitespace();
        self.reject_parentheses()?;

        match self.rest().chars().next() {
            Some('"') => self.parse_string_literal(),
            Some(c) if is_ident_start(c) => {
                let ident = self.parse_identifier()?;
                Ok(ident)
            }
            Some(_) => {
                let (line, column) = self.position();
                Err(QueryParseError::UnexpectedToken {
                    line,
                    column,
                    expected: "string or identifier list value".to_owned(),
                    found: short_snippet(self.rest()),
                })
            }
            None => {
                let (line, column) = self.position();
                Err(QueryParseError::UnexpectedEof {
                    line,
                    column,
                    expected: "string or identifier list value".to_owned(),
                })
            }
        }
    }

    fn parse_identifier(&mut self) -> Result<String, QueryParseError> {
        self.skip_whitespace();
        let start = self.pos;
        match self.rest().chars().next() {
            Some(c) if is_ident_start(c) => {
                self.pos += c.len_utf8();
            }
            Some(_) => {
                let (line, column) = self.position();
                return Err(QueryParseError::UnexpectedToken {
                    line,
                    column,
                    expected: "identifier".to_owned(),
                    found: short_snippet(self.rest()),
                });
            }
            None => {
                let (line, column) = self.position();
                return Err(QueryParseError::UnexpectedEof {
                    line,
                    column,
                    expected: "identifier".to_owned(),
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

    fn parse_string_literal(&mut self) -> Result<String, QueryParseError> {
        self.skip_whitespace();
        let (line, column) = self.position();
        if !self.try_consume_char('"') {
            return Err(QueryParseError::UnexpectedToken {
                line,
                column,
                expected: "`\"`-quoted string literal".to_owned(),
                found: short_snippet(self.rest()),
            });
        }

        let mut out = String::new();
        loop {
            match self.rest().chars().next() {
                Some('"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some('\\') => {
                    self.pos += 1;
                    match self.rest().chars().next() {
                        Some(c @ ('"' | '\\')) => {
                            out.push(c);
                            self.pos += c.len_utf8();
                        }
                        Some(other) => {
                            out.push('\\');
                            out.push(other);
                            self.pos += other.len_utf8();
                        }
                        None => {
                            return Err(QueryParseError::UnterminatedString { line, column });
                        }
                    }
                }
                Some(c) => {
                    out.push(c);
                    self.pos += c.len_utf8();
                }
                None => {
                    return Err(QueryParseError::UnterminatedString { line, column });
                }
            }
        }
    }

    fn parse_integer_literal(&mut self) -> Result<i64, QueryParseError> {
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
            return Err(QueryParseError::InvalidInteger {
                line,
                column,
                literal: self.source[start..self.pos].to_owned(),
            });
        }

        if self.rest().starts_with('.')
            && self.source[self.pos + 1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
        {
            self.pos += 1;
            while let Some(c) = self.rest().chars().next() {
                if c.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            let (line, column) = position_for(self.source, start);
            return Err(QueryParseError::InvalidInteger {
                line,
                column,
                literal: self.source[start..self.pos].to_owned(),
            });
        }

        let literal = &self.source[start..self.pos];
        literal.parse::<i64>().map_err(|_| {
            let (line, column) = position_for(self.source, start);
            QueryParseError::InvalidInteger {
                line,
                column,
                literal: literal.to_owned(),
            }
        })
    }

    fn try_parse_compare_operator(&mut self) -> Option<QueryOperator> {
        if self.try_consume_char_seq("!=") {
            return Some(QueryOperator::Neq);
        }
        if self.try_consume_char_seq("<=") {
            return Some(QueryOperator::Lte);
        }
        if self.try_consume_char_seq(">=") {
            return Some(QueryOperator::Gte);
        }
        if self.try_consume_char('=') {
            return Some(QueryOperator::Eq);
        }
        if self.try_consume_char('<') {
            return Some(QueryOperator::Lt);
        }
        if self.try_consume_char('>') {
            return Some(QueryOperator::Gt);
        }
        None
    }

    fn reject_disallowed_tokens(&mut self) -> Result<(), QueryParseError> {
        self.skip_whitespace();
        for token in ["or", "not"] {
            if self.rest().starts_with(token) {
                let after = self.pos + token.len();
                if !self.source[after..]
                    .chars()
                    .next()
                    .is_some_and(is_ident_continue)
                {
                    let (line, column) = self.position();
                    return Err(QueryParseError::DisallowedToken {
                        line,
                        column,
                        token: token.to_owned(),
                    });
                }
            }
        }
        self.reject_parentheses()
    }

    fn reject_parentheses(&mut self) -> Result<(), QueryParseError> {
        self.skip_whitespace();
        if self.rest().starts_with('(') || self.rest().starts_with(')') {
            let (line, column) = self.position();
            let token = self
                .rest()
                .chars()
                .next()
                .map_or_else(String::new, |c| c.to_string());
            return Err(QueryParseError::DisallowedToken {
                line,
                column,
                token,
            });
        }
        Ok(())
    }

    fn expect_eof(&mut self) -> Result<(), QueryParseError> {
        if self.at_eof() {
            Ok(())
        } else {
            let (line, column) = self.position();
            Err(QueryParseError::UnexpectedToken {
                line,
                column,
                expected: "end of input".to_owned(),
                found: short_snippet(self.rest()),
            })
        }
    }

    fn at_eof(&mut self) -> bool {
        self.skip_whitespace();
        self.pos >= self.source.len()
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.rest().chars().next() {
            if c.is_whitespace() {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
    }

    fn try_consume_keyword(&mut self, token: &str) -> bool {
        self.skip_whitespace();
        if !self.rest().starts_with(token) {
            return false;
        }
        let after = self.pos + token.len();
        if self.source[after..]
            .chars()
            .next()
            .is_some_and(is_ident_continue)
        {
            return false;
        }
        self.pos = after;
        true
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

    fn try_consume_char_seq(&mut self, seq: &str) -> bool {
        self.skip_whitespace();
        if self.rest().starts_with(seq) {
            self.pos += seq.len();
            true
        } else {
            false
        }
    }

    fn rest(&self) -> &'a str {
        &self.source[self.pos..]
    }

    fn position(&self) -> (usize, usize) {
        position_for(self.source, self.pos)
    }
}

const fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

const fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_rfc3339(value: &str) -> bool {
    DateTime::parse_from_rfc3339(value).is_ok()
}

fn position_for(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1_usize;
    let mut column = 1_usize;
    let clamped = offset.min(source.len());
    for c in source[..clamped].chars() {
        if c == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn short_snippet(rest: &str) -> String {
    let trimmed = rest.trim_end();
    let line = trimmed.split('\n').next().unwrap_or("");
    if line.is_empty() {
        "<end of input>".to_owned()
    } else if line.chars().count() > 32 {
        let head: String = line.chars().take(32).collect();
        format!("{head}...")
    } else {
        line.to_owned()
    }
}
