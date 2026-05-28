#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use strum_macros::EnumCount;

/// Closed vocabulary for GPU capability classes (S8.2 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumCount)]
pub enum GpuCapabilityClass {
    RenderOnly,
    ComputeOnly,
    RenderAndCompute,
    VideoEncode,
    VideoDecode,
}

/// Closed vocabulary for GPU vendor classification (S8.2 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumCount)]
pub enum GpuVendorKind {
    Amd,
    Intel,
    Nvidia,
    Arm,
    Apple,
    Other,
}
