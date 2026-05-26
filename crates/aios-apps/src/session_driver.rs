//! S6.5 Session Container Driver — session allocation, lifecycle tracking,
//! and adapter binding.
//!
//! The `SessionDriver` trait is the async contract for session management.
//! `InMemorySessionDriver` binds to the `CompatibilityOrchestrator` (T-118)
//! and tracks each session through its lifecycle states:
//! `Allocating → Active → Suspended → Terminating → Terminated`.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};
use tokio::sync::RwLock;

use crate::compatibility_orchestrator::CompatibilityOrchestrator;
use crate::ecosystem::EcosystemRuntime;
use crate::error::AppsError;
use crate::package::PackageId;
use crate::session::SessionId;

// ---------------------------------------------------------------------------
// SessionState — driver-level lifecycle states
// ---------------------------------------------------------------------------

/// S6.5 — session driver lifecycle states.
///
/// These are the states the session driver tracks, distinct from the
/// `SessionContainerState` enum on `SessionRecord` which tracks the
/// OCI runtime state.
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
pub enum SessionState {
    /// Session is being allocated; adapter binding in progress.
    Allocating,
    /// Session is active; adapter bound and heartbeat current.
    Active,
    /// Session is suspended; resources preserved.
    Suspended,
    /// Session is terminating; resources being released.
    Terminating,
    /// Session is terminated; terminal state.
    Terminated,
}

// ---------------------------------------------------------------------------
// Principal — subject identifier for session requests
// ---------------------------------------------------------------------------

/// The subject requesting a session.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Principal {
    /// Canonical subject identifier (e.g. `human:lucky`).
    pub canonical_id: String,
}

// ---------------------------------------------------------------------------
// CapabilityHandle — granted capability reference
// ---------------------------------------------------------------------------

/// A capability granted to a session at open time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityHandle {
    /// The capability identifier.
    pub capability_id: String,
}

// ---------------------------------------------------------------------------
// SessionMetrics — runtime metrics collected during a session
// ---------------------------------------------------------------------------

/// Runtime metrics collected during a session's lifetime.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMetrics {
    /// Total uptime in seconds.
    pub total_uptime_seconds: u64,
    /// Number of heartbeats received.
    pub heartbeat_count: u64,
}

// ---------------------------------------------------------------------------
// OpenSessionRequest
// ---------------------------------------------------------------------------

/// Request to open a new session container.
#[derive(Clone, Debug)]
pub struct OpenSessionRequest {
    /// The package to run in the session.
    pub package_id: PackageId,
    /// The ecosystem runtime to use.
    pub ecosystem: EcosystemRuntime,
    /// The subject requesting the session.
    pub requester: Principal,
    /// Capabilities granted to the session.
    pub capability_grants: Vec<CapabilityHandle>,
    /// Maximum duration without heartbeat before automatic termination.
    pub timeout: Duration,
}

// ---------------------------------------------------------------------------
// SessionFilter
// ---------------------------------------------------------------------------

/// Filter predicate for `list_sessions`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionFilter {
    /// Return all sessions regardless of state or ownership.
    All,
    /// Return sessions for a specific package.
    ByPackage(PackageId),
    /// Return sessions owned by a specific principal.
    ByPrincipal(Principal),
    /// Return sessions in a specific lifecycle state.
    ByState(SessionState),
}

// ---------------------------------------------------------------------------
// SessionExitReason
// ---------------------------------------------------------------------------

/// Why a session ended.
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
pub enum SessionExitReason {
    /// Owner called `close_session`.
    ClosedByOwner,
    /// Session timed out due to missing heartbeat.
    TimedOut,
    /// Policy kernel revoked the session.
    PolicyRevoked,
    /// Adapter failed during execution.
    AdapterFailure,
    /// Recovery mode reclaimed the session resources.
    RecoveryReclaim,
    /// Session process crashed.
    Crashed,
}

// ---------------------------------------------------------------------------
// SessionTerminationReceipt
// ---------------------------------------------------------------------------

/// Receipt produced when a session is terminated.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTerminationReceipt {
    /// The session that was terminated.
    pub session_id: SessionId,
    /// When the session ended.
    pub ended_at: DateTime<Utc>,
    /// Why the session ended.
    pub exit_reason: SessionExitReason,
    /// Final metrics for the session.
    pub final_metrics: SessionMetrics,
}

// ---------------------------------------------------------------------------
// SessionDescriptor — public-facing session view
// ---------------------------------------------------------------------------

/// Public-facing session descriptor returned by the driver.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionDescriptor {
    /// Canonical session identifier (`sess_<ulid26>`).
    pub session_id: SessionId,
    /// The package running in this session.
    pub package_id: PackageId,
    /// The ecosystem runtime backing this session.
    pub ecosystem: EcosystemRuntime,
    /// Current lifecycle state.
    pub state: SessionState,
    /// Who requested the session.
    pub requester: Principal,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the last heartbeat was received.
    pub last_heartbeat: DateTime<Utc>,
    /// Session timeout in seconds.
    pub timeout_seconds: u64,
}

// ---------------------------------------------------------------------------
// SessionEntry — internal storage record
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct SessionEntry {
    session_id: SessionId,
    package_id: PackageId,
    ecosystem: EcosystemRuntime,
    state: SessionState,
    requester: Principal,
    #[allow(dead_code)]
    capability_grants: Vec<CapabilityHandle>,
    created_at: DateTime<Utc>,
    last_heartbeat: DateTime<Utc>,
    timeout: Duration,
    heartbeat_count: u64,
}

impl SessionEntry {
    fn to_descriptor(&self) -> SessionDescriptor {
        SessionDescriptor {
            session_id: self.session_id.clone(),
            package_id: self.package_id.clone(),
            ecosystem: self.ecosystem,
            state: self.state,
            requester: self.requester.clone(),
            created_at: self.created_at,
            last_heartbeat: self.last_heartbeat,
            timeout_seconds: self.timeout.as_secs(),
        }
    }

    fn is_timed_out(&self, now: DateTime<Utc>) -> bool {
        let elapsed = (now - self.last_heartbeat).num_seconds();
        if elapsed < 0 {
            return false;
        }
        elapsed.unsigned_abs() >= self.timeout.as_secs()
    }

    fn compute_metrics(&self, ended_at: DateTime<Utc>) -> SessionMetrics {
        let uptime = (ended_at - self.created_at).num_seconds();
        SessionMetrics {
            total_uptime_seconds: uptime.max(0).unsigned_abs(),
            heartbeat_count: self.heartbeat_count,
        }
    }
}

// ---------------------------------------------------------------------------
// SessionDriver trait
// ---------------------------------------------------------------------------

/// S6.5 — the async contract for session container management.
///
/// The session driver allocates per-app sessions, binds them to ecosystem
/// adapters through the `CompatibilityOrchestrator`, tracks lifecycle state,
/// and produces `SessionTerminationReceipt` on close.
#[async_trait]
pub trait SessionDriver: Send + Sync {
    /// Open a new session and bind it to an ecosystem adapter.
    ///
    /// Returns `Active` session descriptor on success.
    ///
    /// # Errors
    ///
    /// Returns `AppsError` when the ecosystem has no registered adapter.
    async fn open_session(&self, req: OpenSessionRequest) -> Result<SessionDescriptor, AppsError>;

    /// Close a session and produce a termination receipt.
    ///
    /// Once closed, the session transitions to `Terminated` and further
    /// operations on it return `SessionNotFound`.
    ///
    /// # Errors
    ///
    /// Returns `SessionNotFound` when the session does not exist or is
    /// already terminated.
    async fn close_session(&self, id: SessionId) -> Result<SessionTerminationReceipt, AppsError>;

    /// Get a session descriptor by id.
    ///
    /// If the session has timed out, its state is updated to `Terminated`
    /// before returning.
    ///
    /// # Errors
    ///
    /// Returns `SessionNotFound` when the session does not exist.
    async fn get_session(&self, id: SessionId) -> Result<SessionDescriptor, AppsError>;

    /// List sessions matching a filter predicate.
    async fn list_sessions(&self, filter: SessionFilter) -> Vec<SessionDescriptor>;

    /// Send a heartbeat to keep the session alive.
    ///
    /// Updates `last_heartbeat` and increments the heartbeat counter.
    /// If the session has already timed out, transitions it to `Terminated`.
    ///
    /// # Errors
    ///
    /// Returns `SessionNotFound` when the session does not exist or has
    /// already timed out.
    async fn heartbeat(&self, id: SessionId) -> Result<(), AppsError>;
}

// ---------------------------------------------------------------------------
// InMemorySessionDriver
// ---------------------------------------------------------------------------

/// In-memory `SessionDriver` backed by `RwLock<HashMap<SessionId, SessionEntry>>`.
///
/// Binds to a `CompatibilityOrchestrator` to validate ecosystem adapter
/// availability at session open time.
pub struct InMemorySessionDriver {
    sessions: RwLock<HashMap<SessionId, SessionEntry>>,
    orchestrator: CompatibilityOrchestrator,
}

impl InMemorySessionDriver {
    /// Create a driver with the given orchestrator.
    #[must_use]
    pub fn new(orchestrator: CompatibilityOrchestrator) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            orchestrator,
        }
    }

    /// Create a driver pre-bound to the default orchestrator
    /// (all five stub adapters registered).
    #[must_use]
    pub fn new_with_defaults() -> Self {
        Self::new(CompatibilityOrchestrator::new_with_defaults())
    }
}

impl std::fmt::Debug for InMemorySessionDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemorySessionDriver")
            .field("orchestrator", &self.orchestrator)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl SessionDriver for InMemorySessionDriver {
    async fn open_session(&self, req: OpenSessionRequest) -> Result<SessionDescriptor, AppsError> {
        // Verify the ecosystem has a registered adapter.
        let ecosystems = self.orchestrator.registered_ecosystems();
        if !ecosystems.contains(&req.ecosystem) {
            return Err(AppsError::ProfileNotFound {
                app_id: String::new(),
                runtime: req.ecosystem.to_string(),
            });
        }

        let session_id = SessionId(format!(
            "sess_{}",
            ulid::Ulid::new().to_string().to_lowercase()
        ));
        let now = Utc::now();

        let entry = SessionEntry {
            session_id: session_id.clone(),
            package_id: req.package_id,
            ecosystem: req.ecosystem,
            state: SessionState::Active,
            requester: req.requester,
            capability_grants: req.capability_grants,
            created_at: now,
            last_heartbeat: now,
            timeout: req.timeout,
            heartbeat_count: 0,
        };

        let descriptor = entry.to_descriptor();
        self.sessions.write().await.insert(session_id, entry);
        Ok(descriptor)
    }

    async fn close_session(&self, id: SessionId) -> Result<SessionTerminationReceipt, AppsError> {
        let mut guard = self.sessions.write().await;
        let entry = guard
            .get_mut(&id)
            .ok_or_else(|| AppsError::SessionNotFound(id.0.clone()))?;

        if entry.state == SessionState::Terminated {
            return Err(AppsError::SessionNotFound(id.0.clone()));
        }

        entry.state = SessionState::Terminated;
        let ended_at = Utc::now();
        let metrics = entry.compute_metrics(ended_at);
        drop(guard);

        Ok(SessionTerminationReceipt {
            session_id: id,
            ended_at,
            exit_reason: SessionExitReason::ClosedByOwner,
            final_metrics: metrics,
        })
    }

    async fn get_session(&self, id: SessionId) -> Result<SessionDescriptor, AppsError> {
        let mut guard = self.sessions.write().await;
        let entry = guard
            .get_mut(&id)
            .ok_or_else(|| AppsError::SessionNotFound(id.0.clone()))?;

        // Lazy timeout check: if the session has exceeded its heartbeat
        // window, transition to Terminated.
        if entry.state != SessionState::Terminated && entry.is_timed_out(Utc::now()) {
            entry.state = SessionState::Terminated;
        }

        let desc = entry.to_descriptor();
        drop(guard);
        Ok(desc)
    }

    async fn list_sessions(&self, filter: SessionFilter) -> Vec<SessionDescriptor> {
        let guard = self.sessions.read().await;
        guard
            .values()
            .filter(|entry| match &filter {
                SessionFilter::All => true,
                SessionFilter::ByPackage(pkg) => entry.package_id == *pkg,
                SessionFilter::ByPrincipal(principal) => entry.requester == *principal,
                SessionFilter::ByState(state) => entry.state == *state,
            })
            .map(SessionEntry::to_descriptor)
            .collect()
    }

    async fn heartbeat(&self, id: SessionId) -> Result<(), AppsError> {
        let mut guard = self.sessions.write().await;
        let entry = guard
            .get_mut(&id)
            .ok_or_else(|| AppsError::SessionNotFound(id.0.clone()))?;

        if entry.state == SessionState::Terminated {
            return Err(AppsError::SessionNotFound(id.0.clone()));
        }

        // Check timeout before accepting heartbeat.
        if entry.is_timed_out(Utc::now()) {
            entry.state = SessionState::Terminated;
            return Err(AppsError::SessionNotFound(id.0.clone()));
        }

        entry.last_heartbeat = Utc::now();
        entry.heartbeat_count += 1;
        drop(guard);
        Ok(())
    }
}
