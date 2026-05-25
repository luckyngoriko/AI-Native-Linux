//! gRPC client wiring for renderer-side access to AIOS backend services.

pub mod aios_client;
pub mod endpoint;
pub mod in_process;

pub use aios_client::AiosClient;
pub use endpoint::AiosEndpoints;
pub use in_process::{InProcessBackend, ShutdownHandle};
