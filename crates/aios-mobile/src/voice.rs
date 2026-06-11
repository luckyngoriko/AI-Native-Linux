//! Voice surface — a mobile surface that handles speech-to-text,
//! text-to-speech, and voice intent classification with mandatory visual
//! confirmation for high-risk actions.

/// A voice-capable mobile surface that can receive, classify, and map
/// voice intents to typed action requests. High-risk intents always require
/// visual confirmation before dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceSurface {
    /// Unique surface identifier (format `vsrf_<ULID>`).
    pub surface_id: String,
    /// Whether speech-to-text (STT) is enabled on this surface.
    pub stt_enabled: bool,
    /// Whether text-to-speech (TTS) is enabled on this surface.
    pub tts_enabled: bool,
    /// Always `true` — high-risk voice intents require visual confirmation
    /// on the surface screen.
    pub high_risk_requires_visual: bool,
}

impl VoiceSurface {
    /// Creates a new voice surface with the given STT/TTS capabilities.
    #[must_use]
    pub fn new(stt_enabled: bool, tts_enabled: bool) -> Self {
        let surface_id = format!("vsrf_{}", ulid::Ulid::new());
        Self {
            surface_id,
            stt_enabled,
            tts_enabled,
            high_risk_requires_visual: true,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn constructor_creates_voice_surface() {
        let vs = VoiceSurface::new(true, true);
        assert!(vs.surface_id.starts_with("vsrf_"));
        assert!(vs.stt_enabled);
        assert!(vs.tts_enabled);
        assert!(vs.high_risk_requires_visual);
    }

    #[test]
    fn high_risk_always_requires_visual() {
        let vs_full = VoiceSurface::new(true, true);
        let vs_minimal = VoiceSurface::new(false, false);
        assert!(vs_full.high_risk_requires_visual);
        assert!(vs_minimal.high_risk_requires_visual);
    }

    #[test]
    fn surface_ids_are_unique() {
        let vs1 = VoiceSurface::new(true, false);
        let vs2 = VoiceSurface::new(false, true);
        assert_ne!(vs1.surface_id, vs2.surface_id);
    }
}
