//! Per-group iframe origin scheme and verification (S7.5 §I4).
//!
//! Every group gets a unique origin of the form
//! `https://<token>.aios.localhost:<port>`. Recovery surfaces use
//! `https://recovery.localhost:<port>`. The AIOS chrome surface uses
//! `https://aios.localhost:<port>`.

use crate::error::WebRendererError;
use serde::{Deserialize, Serialize};

/// An origin token — the subdomain segment used in per-group iframe origins.
///
/// Examples: `"acme-app"`, `"aios"`, `"recovery"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OriginToken(pub String);

/// The scheme of a parsed origin — determines what kind of surface this origin
/// belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OriginScheme {
    /// `https://<token>.aios.localhost:<port>` — per-group application origin.
    AiosLocalhost(OriginToken),
    /// `https://recovery.localhost:<port>` — recovery shell surface.
    Recovery,
    /// `https://aios.localhost:<port>` — AIOS chrome surface.
    AppOrigin(OriginToken),
}

/// A parsed origin composed of its scheme, host, port, and full origin string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedOrigin {
    /// The parsed origin scheme.
    pub scheme: OriginScheme,
    /// The host segment of the origin.
    pub host: String,
    /// The port number.
    pub port: u16,
    /// The full origin string as presented (e.g.
    /// `"https://acme-app.aios.localhost:8443"`).
    pub full_origin: String,
}

impl OriginScheme {
    /// Parse an origin string into a `ParsedOrigin`.
    ///
    /// Recognized patterns:
    /// - `https://<token>.aios.localhost:<port>` → `AiosLocalhost`
    /// - `https://recovery.localhost:<port>` → `Recovery`
    /// - `https://aios.localhost:<port>` → `AppOrigin`
    ///
    /// # Errors
    ///
    /// Returns `WebRendererError::Internal` if the origin string does not match
    /// any recognized pattern.
    pub fn parse(origin: &str) -> Result<ParsedOrigin, WebRendererError> {
        // Strip scheme: "https://"
        let rest = origin.strip_prefix("https://").ok_or_else(|| {
            WebRendererError::Internal(format!("origin must start with https://: got '{origin}'"))
        })?;

        // Split host and optional port
        let (host, port) = if let Some(colon_pos) = rest.rfind(':') {
            let host_part = &rest[..colon_pos];
            let port_part = &rest[colon_pos + 1..];
            let port: u16 = port_part.parse().map_err(|_| {
                WebRendererError::Internal(format!(
                    "invalid port in origin '{origin}': '{port_part}'"
                ))
            })?;
            (host_part.to_string(), port)
        } else {
            // Default HTTPS port
            (rest.to_string(), 443_u16)
        };

        // Match host patterns
        let full_origin = origin.to_string();

        if host == "recovery.localhost" {
            return Ok(ParsedOrigin {
                scheme: Self::Recovery,
                host,
                port,
                full_origin,
            });
        }

        if host == "aios.localhost" {
            return Ok(ParsedOrigin {
                scheme: Self::AppOrigin(OriginToken("aios".to_string())),
                host,
                port,
                full_origin,
            });
        }

        // Per-group: "<token>.aios.localhost"
        if let Some(token_part) = host.strip_suffix(".aios.localhost") {
            if token_part.is_empty() || token_part == "aios" || token_part == "recovery" {
                return Err(WebRendererError::Internal(format!(
                    "invalid origin token in '{origin}': token '{token_part}' is reserved"
                )));
            }
            return Ok(ParsedOrigin {
                scheme: Self::AiosLocalhost(OriginToken(token_part.to_string())),
                host,
                port,
                full_origin,
            });
        }

        Err(WebRendererError::Internal(format!(
            "unrecognized origin host pattern: '{host}' in '{origin}'"
        )))
    }
}

impl ParsedOrigin {
    /// Verify that the parsed origin matches the expected group identifier
    /// (INV I4).
    ///
    /// For `AiosLocalhost` origins, the token must match `expected_group_id`.
    /// For `Recovery` and `AppOrigin` origins, verification is deferred to the
    /// recovery shell and chrome service respectively — these always pass here.
    ///
    /// # Errors
    ///
    /// Returns `WebRendererError::OriginVerificationFailed` if the origin token
    /// does not match the expected group.
    pub fn verify_against_group(&self, expected_group_id: &str) -> Result<(), WebRendererError> {
        match &self.scheme {
            OriginScheme::AiosLocalhost(token) => {
                if token.0 != expected_group_id {
                    return Err(WebRendererError::OriginVerificationFailed {
                        expected_group_id: expected_group_id.to_string(),
                        presented_origin: self.full_origin.clone(),
                    });
                }
                Ok(())
            }
            OriginScheme::Recovery | OriginScheme::AppOrigin(_) => {
                // Recovery and chrome origins are verified by their respective
                // services; they always pass the group-level check.
                Ok(())
            }
        }
    }
}
