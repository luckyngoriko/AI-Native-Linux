//! Hand-written recursive-descent parser for S2.4 verification expressions.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S2.4 verification grammar vocabulary"
)]

use serde_json::{Map, Number, Value};

use crate::{
    PrimitiveInvocation, VerificationDuration, VerificationDurationUnit, VerificationError,
    VerificationGrammar, VerificationPrimitive,
};

const MAX_COMPOSITION_DEPTH: usize = 8;

/// Parse S2.4 verification expression source into a typed AST.
///
/// # Errors
///
/// Returns [`VerificationError::IntentParseFailed`] when the source is not in
/// the closed S2.4 grammar, uses an unknown primitive, exceeds the composition
/// depth limit, or omits a required primitive arg.
pub fn parse(source: &str) -> Result<VerificationGrammar, VerificationError> {
    let mut parser = Parser::new(source);
    let expression = parser.parse_expression(1)?;
    parser.expect_eof()?;
    Ok(expression)
}

struct Parser<'a> {
    source: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    const fn new(source: &'a str) -> Self {
        Self { source, pos: 0 }
    }

    fn parse_expression(&mut self, depth: usize) -> Result<VerificationGrammar, VerificationError> {
        if depth > MAX_COMPOSITION_DEPTH {
            return Err(self.error_here("composition depth exceeds S2.4 limit of 8"));
        }

        self.skip_whitespace();
        if self.at_raw_eof() {
            return Err(self.error_here("unexpected end of input: expected expression"));
        }
        self.reject_infix_words()?;
        self.reject_grouping()?;

        if self.next_keyword_is("all") {
            self.parse_terms_composition("all", depth)
                .map(VerificationGrammar::All)
        } else if self.next_keyword_is("any") {
            self.parse_terms_composition("any", depth)
                .map(VerificationGrammar::Any)
        } else if self.next_keyword_is("not") {
            self.parse_not(depth)
        } else if self.next_keyword_is("eventually") {
            self.parse_eventually(depth)
        } else {
            self.parse_primitive_call()
        }
    }

    fn parse_terms_composition(
        &mut self,
        combinator: &str,
        depth: usize,
    ) -> Result<Vec<VerificationGrammar>, VerificationError> {
        let start = self.pos;
        self.consume_keyword(combinator)?;
        self.expect_char('[', "expected `[` after composition keyword")?;

        let mut terms = Vec::new();
        self.skip_whitespace();
        if self.try_consume_char(']') {
            return Err(self.error_at(start, format!("{combinator} requires at least 2 terms")));
        }

        loop {
            terms.push(self.parse_expression(depth + 1)?);
            self.skip_whitespace();
            if self.try_consume_char(']') {
                break;
            }
            self.expect_char(',', "expected `,` or `]` in composition")?;
            self.skip_whitespace();
            if self.rest().starts_with(']') {
                return Err(self.error_here("unexpected `]`: expected expression after `,`"));
            }
        }

        if terms.len() < 2 {
            return Err(self.error_at(start, format!("{combinator} requires at least 2 terms")));
        }

        Ok(terms)
    }

    fn parse_not(&mut self, depth: usize) -> Result<VerificationGrammar, VerificationError> {
        self.consume_keyword("not")?;
        self.expect_char('(', "expected `(` after `not`")?;
        let term = self.parse_expression(depth + 1)?;
        self.expect_char(')', "expected `)` after `not` term")?;
        Ok(VerificationGrammar::Not(Box::new(term)))
    }

    fn parse_eventually(&mut self, depth: usize) -> Result<VerificationGrammar, VerificationError> {
        self.consume_keyword("eventually")?;
        self.expect_char('(', "expected `(` after `eventually`")?;
        let term = self.parse_expression(depth + 1)?;
        self.expect_char(',', "expected `, max_duration=...` in `eventually`")?;
        self.consume_keyword("max_duration")?;
        self.expect_char('=', "expected `=` after `max_duration`")?;
        let max_duration = self.parse_duration()?;
        self.expect_char(',', "expected `, interval=...` in `eventually`")?;
        self.consume_keyword("interval")?;
        self.expect_char('=', "expected `=` after `interval`")?;
        let interval = self.parse_duration()?;
        self.expect_char(')', "expected `)` after `eventually`")?;
        Ok(VerificationGrammar::Eventually {
            term: Box::new(term),
            max_duration,
            interval,
        })
    }

    fn parse_primitive_call(&mut self) -> Result<VerificationGrammar, VerificationError> {
        let start = self.pos;
        let primitive_name = self.parse_primitive_name()?;
        let kind = parse_primitive_name(&primitive_name).ok_or_else(|| {
            self.error_at(
                start,
                format!(
                    "unknown verification primitive `{primitive_name}` (closed S2.4 vocabulary)"
                ),
            )
        })?;
        self.expect_char('(', "expected `(` after primitive name")?;
        let args = self.parse_args(kind)?;
        self.expect_char(')', "expected `)` after primitive args")?;
        validate_required_args(kind, &args, self.source, start, &primitive_name)?;
        Ok(VerificationGrammar::Primitive(PrimitiveInvocation {
            kind,
            args: Value::Object(args),
        }))
    }

    fn parse_args(
        &mut self,
        kind: VerificationPrimitive,
    ) -> Result<Map<String, Value>, VerificationError> {
        let mut args = Map::new();
        self.skip_whitespace();
        if self.rest().starts_with(')') {
            return Ok(args);
        }

        loop {
            let raw_key = self.parse_identifier()?;
            let key = canonical_arg_name(kind, &raw_key);
            self.expect_char('=', "expected `=` after arg name")?;
            let value = self.parse_value()?;
            if args.insert(key.clone(), value).is_some() {
                return Err(self.error_here(format!("duplicate arg `{key}`")));
            }
            self.skip_whitespace();
            if self.rest().starts_with(')') {
                break;
            }
            self.expect_char(',', "expected `,` or `)` after arg value")?;
        }

        Ok(args)
    }

    fn parse_value(&mut self) -> Result<Value, VerificationError> {
        self.skip_whitespace();
        match self.rest().chars().next() {
            Some('"' | '\'') => self.parse_string_literal().map(Value::String),
            Some('-' | '0'..='9') => self.parse_integer_literal().map(Value::Number),
            Some(c) if is_ident_start(c) => {
                let ident = self.parse_identifier()?;
                match ident.as_str() {
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    "null" => Ok(Value::Null),
                    _ => Ok(Value::String(ident)),
                }
            }
            Some(found) => {
                Err(self.error_here(format!("unexpected token `{found}`: expected value")))
            }
            None => Err(self.error_here("unexpected end of input: expected value")),
        }
    }

    fn parse_duration(&mut self) -> Result<VerificationDuration, VerificationError> {
        self.skip_whitespace();
        let start = self.pos;
        let value = self.parse_u64_digits("duration")?;
        let unit = if self.try_consume_raw("ms") {
            VerificationDurationUnit::Milliseconds
        } else if self.try_consume_raw("s") {
            VerificationDurationUnit::Seconds
        } else if self.try_consume_raw("m") {
            VerificationDurationUnit::Minutes
        } else if self.try_consume_raw("h") {
            VerificationDurationUnit::Hours
        } else {
            return Err(self.error_at(
                start,
                "invalid duration: expected unit `ms`, `s`, `m`, or `h`",
            ));
        };
        Ok(VerificationDuration { value, unit })
    }

    fn parse_string_literal(&mut self) -> Result<String, VerificationError> {
        self.skip_whitespace();
        let start = self.pos;
        let Some(quote @ ('"' | '\'')) = self.rest().chars().next() else {
            return Err(self.error_here("expected string literal"));
        };
        self.pos += quote.len_utf8();
        let mut output = String::new();

        while let Some(character) = self.rest().chars().next() {
            self.pos += character.len_utf8();
            if character == quote {
                return Ok(output);
            }
            if character == '\n' {
                return Err(self.error_at(start, "unterminated string literal"));
            }
            if character == '\\' {
                let Some(escaped) = self.rest().chars().next() else {
                    return Err(self.error_at(start, "unterminated string literal"));
                };
                self.pos += escaped.len_utf8();
                match escaped {
                    '"' => output.push('"'),
                    '\'' => output.push('\''),
                    '\\' => output.push('\\'),
                    'n' => output.push('\n'),
                    'r' => output.push('\r'),
                    't' => output.push('\t'),
                    other => {
                        output.push('\\');
                        output.push(other);
                    }
                }
            } else {
                output.push(character);
            }
        }

        Err(self.error_at(start, "unterminated string literal"))
    }

    fn parse_integer_literal(&mut self) -> Result<Number, VerificationError> {
        self.skip_whitespace();
        let start = self.pos;
        let negative = self.try_consume_char('-');
        let value = self.parse_u64_digits("integer literal")?;
        if self.rest().chars().next().is_some_and(is_ident_start) {
            let suffix = self.parse_identifier()?;
            return Err(self.error_at(start, format!("invalid integer literal `{value}{suffix}`")));
        }
        if negative {
            let signed = i64::try_from(value)
                .ok()
                .and_then(i64::checked_neg)
                .ok_or_else(|| {
                    self.error_at(start, format!("invalid integer literal `-{value}`"))
                })?;
            Ok(Number::from(signed))
        } else {
            Ok(Number::from(value))
        }
    }

    fn parse_u64_digits(&mut self, label: &str) -> Result<u64, VerificationError> {
        self.skip_whitespace();
        let start = self.pos;
        while self
            .rest()
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
        {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(self.error_at(start, format!("expected {label}")));
        }
        self.source[start..self.pos]
            .parse::<u64>()
            .map_err(|_err| self.error_at(start, format!("invalid {label}")))
    }

    fn parse_primitive_name(&mut self) -> Result<String, VerificationError> {
        let start = self.pos;
        let mut name = self.parse_identifier()?;
        loop {
            let saved = self.pos;
            if !self.try_consume_char('.') {
                break;
            }
            match self.rest().chars().next() {
                Some(c) if is_ident_start(c) => {
                    name.push('.');
                    name.push_str(&self.parse_identifier()?);
                }
                _ => {
                    self.pos = saved;
                    break;
                }
            }
        }
        if name.is_empty() {
            Err(self.error_at(start, "expected primitive name"))
        } else {
            Ok(name)
        }
    }

    fn parse_identifier(&mut self) -> Result<String, VerificationError> {
        self.skip_whitespace();
        let start = self.pos;
        let Some(first) = self.rest().chars().next() else {
            return Err(self.error_here("unexpected end of input: expected identifier"));
        };
        if !is_ident_start(first) {
            return Err(self.error_here("expected identifier"));
        }
        self.pos += first.len_utf8();
        while let Some(character) = self.rest().chars().next() {
            if is_ident_continue(character) {
                self.pos += character.len_utf8();
            } else {
                break;
            }
        }
        Ok(self.source[start..self.pos].to_owned())
    }

    fn expect_eof(&mut self) -> Result<(), VerificationError> {
        self.skip_whitespace();
        if self.at_raw_eof() {
            Ok(())
        } else {
            self.reject_infix_words()?;
            Err(self.error_here(format!(
                "unexpected token `{}`: expected end of input",
                short_snippet(self.rest())
            )))
        }
    }

    fn consume_keyword(&mut self, keyword: &str) -> Result<(), VerificationError> {
        self.skip_whitespace();
        if self.next_keyword_is(keyword) {
            self.pos += keyword.len();
            Ok(())
        } else {
            Err(self.error_here(format!("expected `{keyword}`")))
        }
    }

    fn expect_char(&mut self, expected: char, message: &str) -> Result<(), VerificationError> {
        self.skip_whitespace();
        if self.try_consume_char(expected) {
            Ok(())
        } else if self.at_raw_eof() {
            Err(self.error_here(format!("unexpected end of input: {message}")))
        } else {
            Err(self.error_here(message))
        }
    }

    fn try_consume_char(&mut self, expected: char) -> bool {
        self.skip_whitespace();
        if self.rest().starts_with(expected) {
            self.pos += expected.len_utf8();
            true
        } else {
            false
        }
    }

    fn try_consume_raw(&mut self, token: &str) -> bool {
        if self.rest().starts_with(token) {
            self.pos += token.len();
            true
        } else {
            false
        }
    }

    fn next_keyword_is(&mut self, keyword: &str) -> bool {
        self.skip_whitespace();
        if !self.rest().starts_with(keyword) {
            return false;
        }
        let after = self.pos + keyword.len();
        !self.source[after..]
            .chars()
            .next()
            .is_some_and(is_ident_continue)
    }

    fn reject_infix_words(&mut self) -> Result<(), VerificationError> {
        self.skip_whitespace();
        for token in ["and", "or"] {
            if self.next_keyword_is(token) {
                return Err(self.error_here(format!(
                    "disallowed grammar token `{token}`: S2.4 uses `all[...]` / `any[...]`, not infix operators"
                )));
            }
        }
        Ok(())
    }

    fn reject_grouping(&mut self) -> Result<(), VerificationError> {
        self.skip_whitespace();
        if self.rest().starts_with('(') {
            Err(self.error_here(
                "disallowed grammar token `(`: parenthesized grouping is not in S2.4 grammar",
            ))
        } else {
            Ok(())
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(character) = self.rest().chars().next() {
            if character.is_whitespace() {
                self.pos += character.len_utf8();
            } else {
                break;
            }
        }
    }

    fn rest(&self) -> &'a str {
        &self.source[self.pos..]
    }

    const fn at_raw_eof(&self) -> bool {
        self.pos >= self.source.len()
    }

    fn error_here(&self, message: impl Into<String>) -> VerificationError {
        self.error_at(self.pos, message)
    }

    fn error_at(&self, pos: usize, message: impl Into<String>) -> VerificationError {
        parse_error(self.source, pos, message)
    }
}

fn parse_primitive_name(token: &str) -> Option<VerificationPrimitive> {
    let wire = token.replace('.', "_").to_ascii_uppercase();
    serde_json::from_value(Value::String(wire)).ok()
}

fn validate_required_args(
    kind: VerificationPrimitive,
    args: &Map<String, Value>,
    source: &str,
    pos: usize,
    primitive_name: &str,
) -> Result<(), VerificationError> {
    for required in required_args(kind) {
        if !args.contains_key(*required) {
            return Err(parse_error(
                source,
                pos,
                format!("missing required arg `{required}` for primitive `{primitive_name}`"),
            ));
        }
    }
    Ok(())
}

const fn required_args(kind: VerificationPrimitive) -> &'static [&'static str] {
    match kind {
        VerificationPrimitive::ServiceActive | VerificationPrimitive::ServiceInactive => {
            &["service"]
        }
        VerificationPrimitive::PackageInstalled => &["package"],
        VerificationPrimitive::PortOpen | VerificationPrimitive::PortClosed => {
            &["host", "port", "protocol"]
        }
        VerificationPrimitive::HttpOk => &["url"],
        VerificationPrimitive::FileExists => &["object_or_path"],
        VerificationPrimitive::FileHash => &["object_or_path", "expected_hash_hex"],
        VerificationPrimitive::RepoExists => &["path_or_object"],
        VerificationPrimitive::AiosfsPointer => {
            &["object_id", "pointer_kind", "expected_version_id"]
        }
        VerificationPrimitive::PolicyDecision => &["policy_decision_id", "expected_decision"],
        VerificationPrimitive::EvidenceExists => &["receipt_id"],
        VerificationPrimitive::NetworkSubjectOutboundClass
        | VerificationPrimitive::NetworkExternalModelCallBrokeredOnly => &["subject_id"],
        VerificationPrimitive::NetworkActiveExposureClass => &["surface_id", "expected_class"],
        VerificationPrimitive::DnsResolverBackend => {
            &["host_id", "expected_backend", "expected_transport"]
        }
        VerificationPrimitive::VpnTunnelActive => &["tunnel_id", "expected_kind"],
        VerificationPrimitive::MdnsPosture => &["host_id", "expected_posture"],
        VerificationPrimitive::AiosfsPathInNamespace => &["path", "expected_scope"],
        VerificationPrimitive::SurfaceInZone => &["surface_id", "expected_zone"],
        VerificationPrimitive::TreeContainsKind => &["tree_id", "kind", "must_contain"],
        VerificationPrimitive::TreeMaxDepth => &["tree_id", "max_depth"],
        VerificationPrimitive::ThemeSatisfiesInvariants
        | VerificationPrimitive::ThemeConstitutionalIconsIntact => &["theme_id"],
        VerificationPrimitive::GpuBindingClass => &["binding_id", "expected_class"],
        VerificationPrimitive::WebRendererBoundTo => &["host", "port"],
        VerificationPrimitive::WebChromeZIndexAtLeast => &["minimum_z_index"],
        VerificationPrimitive::AiosfsPathOwnerResolved => {
            &["path", "expected_owner_subject_id", "namespace_catalog_id"]
        }
        VerificationPrimitive::AiosfsPathRecoveryTreatmentSet => {
            &["path", "expected_treatment", "namespace_catalog_id"]
        }
        VerificationPrimitive::NamespaceCatalogVersion => {
            &["expected_catalog_id", "require_exact_match"]
        }
        VerificationPrimitive::StatusIndicatorVisible => &["indicator", "require_chrome_zone"],
        VerificationPrimitive::SubjectSessionFlagState => &[
            "subject_canonical_id",
            "session_id",
            "flag",
            "expected_state",
        ],
        VerificationPrimitive::FilesystemRootIntact => &["root"],
        VerificationPrimitive::SpecConsumesTable => &["spec_id"],
        VerificationPrimitive::ApprovalBindingState => &["approval_id"],
        VerificationPrimitive::SecretPatternMatch => &["record_id", "pattern_catalog_id"],
    }
}

fn canonical_arg_name(kind: VerificationPrimitive, raw: &str) -> String {
    match (kind, raw) {
        (VerificationPrimitive::ServiceActive | VerificationPrimitive::ServiceInactive, "name") => {
            "service".to_owned()
        }
        (VerificationPrimitive::FileExists | VerificationPrimitive::FileHash, "path") => {
            "object_or_path".to_owned()
        }
        (VerificationPrimitive::FileHash, "expected_hash") => "expected_hash_hex".to_owned(),
        (VerificationPrimitive::RepoExists, "path") => "path_or_object".to_owned(),
        _ => raw.to_owned(),
    }
}

fn parse_error(source: &str, pos: usize, message: impl Into<String>) -> VerificationError {
    let (line, column) = position_for(source, pos);
    VerificationError::IntentParseFailed(format!(
        "{} at line {line}, column {column}",
        message.into()
    ))
}

fn position_for(source: &str, pos: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    for (index, character) in source.char_indices() {
        if index >= pos {
            break;
        }
        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn short_snippet(rest: &str) -> String {
    rest.chars().take(24).collect()
}

const fn is_ident_start(character: char) -> bool {
    character == '_' || character.is_ascii_alphabetic()
}

const fn is_ident_continue(character: char) -> bool {
    character == '_' || character.is_ascii_alphanumeric()
}
