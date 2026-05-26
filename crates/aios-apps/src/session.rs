//! S6.5 Session Container Model — closed enums and newtype identifiers.
//!
//! Defines the session container lifecycle: modes, states, runtimes,
//! streaming protocols, and failure classes for operator-facing desktop
//! and single-app session containers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Canonical session container identifier. Format: `sess_<ulid26>`.
///
/// S6.5 §3 — the session id is the primary key for all session container
/// lifecycle records and evidence emissions.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

// ---------------------------------------------------------------------------
// Closed enums — S6.5 §3.1–§3.5
// ---------------------------------------------------------------------------

/// S6.5 §3.1 — the session container mode. Two values.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
    strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionContainerMode {
    /// Full KDE Plasma desktop in a container; operator connects via browser.
    FullDesktop,
    /// Single application streamed; only one app surface in the container.
    SingleApp,
}

/// S6.5 §3.2 — the session container lifecycle state. Five values.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
    strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionContainerState {
    /// Manifest validated but OCI runtime not yet invoked.
    Idle,
    /// OCI runtime invoked; container booting.
    Starting,
    /// Container running; operator connected via stream.
    Active,
    /// Operator disconnected; container suspended awaiting resume.
    Paused,
    /// Container terminated and resources released; terminal.
    Reclaimed,
}

/// S6.5 §3.3 — the OCI-compatible container runtime backing the session. Two values.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
    strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionContainerRuntime {
    /// Podman (rootless by default; recovery container requires Podman).
    Podman,
    /// Docker (must verify daemon health at adapter start).
    Docker,
}

/// S6.5 §3.4 — the streaming protocol between browser and session container. Two values.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
    strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StreamProtocol {
    /// WebSocket-based streaming (selkies-gstreamer default).
    Websocket,
    /// WebRTC-based streaming with STUN/TURN.
    Webrtc,
}

/// S6.5 §3.5 — the closed failure taxonomy for session container operations.
/// Eight values.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumIter,
    EnumCount,
    strum_macros::Display,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionFailureClass {
    /// Container crashed (OCI runtime or guest kernel panic).
    ContainerCrash,
    /// Stream disconnected (WebSocket close, browser tab navigated away).
    StreamDisconnect,
    /// GPU VRAM budget exhausted; S8.2 reclaim path triggered.
    GpuVramExhausted,
    /// Per-group session quota exceeded; new session creation rejected.
    GroupQuotaExceeded,
    /// S8.1 network policy violation detected in session context.
    NetworkPolicyViolation,
    /// Sandbox escape attempted; container reclaimed and forensic snapshot preserved.
    SandboxEscapeAttempt,
    /// Cross-group filesystem boundary violated at OCI spec validation.
    FilesystemBoundaryViolated,
    /// Renderer refused a STREAMED_SESSION_SURFACE in CHROME zone.
    StreamedSurfaceInChromeBlocked,
}

// ---------------------------------------------------------------------------
// SessionRecord — the top-level struct for a session container
// ---------------------------------------------------------------------------

/// A session container record combining identity, mode, state, runtime,
/// and streaming protocol.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionRecord {
    /// Canonical session identifier (`sess_<ulid26>`).
    pub session_id: SessionId,
    /// The group this session belongs to.
    pub group_id: String,
    /// Session container mode.
    pub mode: SessionContainerMode,
    /// Current lifecycle state.
    pub state: SessionContainerState,
    /// OCI runtime backing the session.
    pub runtime: SessionContainerRuntime,
    /// Streaming protocol in use.
    pub stream_protocol: StreamProtocol,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the state last changed.
    pub state_changed_at: DateTime<Utc>,
}
