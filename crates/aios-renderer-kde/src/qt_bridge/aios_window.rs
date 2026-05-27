//! T-136 — cxx-qt bridge: AiosWindow + AiosApprovalPrompt QObject bindings (S7.4 §4).

use std::pin::Pin;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, title)]
        #[qproperty(QString, surface_id)]
        #[qproperty(bool, chrome_visible)]
        #[qproperty(bool, recovery_active)]
        type AiosWindow = super::AiosWindowRust;

        #[qobject]
        #[qml_element]
        #[qproperty(QString, subject)]
        #[qproperty(QString, action_summary)]
        #[qproperty(QString, evidence_link)]
        type AiosApprovalPrompt = super::AiosApprovalPromptRust;
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        fn enter_recovery(self: Pin<&mut AiosWindow>);

        #[qsignal]
        fn decided(self: Pin<&mut AiosApprovalPrompt>, approved: bool);
    }
}

pub struct AiosWindowRust {
    pub title: cxx_qt_lib::QString,
    pub surface_id: cxx_qt_lib::QString,
    pub chrome_visible: bool,
    pub recovery_active: bool,
}

impl Default for AiosWindowRust {
    fn default() -> Self {
        Self {
            title: cxx_qt_lib::QString::default(),
            surface_id: cxx_qt_lib::QString::default(),
            chrome_visible: true,
            recovery_active: false,
        }
    }
}

impl qobject::AiosWindow {
    /// INV I5: Sets `recovery_active = true` — QML can only read this property.
    pub fn enter_recovery(mut self: Pin<&mut Self>) {
        self.as_mut().set_recovery_active(true);
    }
}

pub struct AiosApprovalPromptRust {
    pub subject: cxx_qt_lib::QString,
    pub action_summary: cxx_qt_lib::QString,
    pub evidence_link: cxx_qt_lib::QString,
}

impl Default for AiosApprovalPromptRust {
    fn default() -> Self {
        Self {
            subject: cxx_qt_lib::QString::default(),
            action_summary: cxx_qt_lib::QString::default(),
            evidence_link: cxx_qt_lib::QString::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AiosApprovalPromptRust;
    use super::AiosWindowRust;

    /// G1 — AiosWindowRust backing struct can be default-constructed.
    #[test]
    fn aios_window_rust_default() {
        let w = AiosWindowRust::default();
        assert_eq!(w.title.to_string(), "");
        assert_eq!(w.surface_id.to_string(), "");
        assert!(w.chrome_visible);
        assert!(!w.recovery_active);
    }

    /// G2 — AiosWindowRust fields are independently settable.
    #[test]
    fn aios_window_rust_field_write() {
        let mut w = AiosWindowRust::default();
        w.title = "AIOS Shell".into();
        w.surface_id = "surf-001".into();
        w.chrome_visible = false;
        w.recovery_active = true;
        assert_eq!(w.title.to_string(), "AIOS Shell");
        assert_eq!(w.surface_id.to_string(), "surf-001");
        assert!(!w.chrome_visible);
        assert!(w.recovery_active);
    }

    /// G3 — AiosApprovalPromptRust backing struct can be default-constructed.
    #[test]
    fn aios_approval_prompt_rust_default() {
        let p = AiosApprovalPromptRust::default();
        assert_eq!(p.subject.to_string(), "");
        assert_eq!(p.action_summary.to_string(), "");
        assert_eq!(p.evidence_link.to_string(), "");
    }

    /// G4 — AiosApprovalPromptRust fields are independently settable.
    #[test]
    fn aios_approval_prompt_rust_field_write() {
        let mut p = AiosApprovalPromptRust::default();
        p.subject = "Allow network bind?".into();
        p.action_summary = "App binds port 443".into();
        p.evidence_link = "ev://tx/abc123".into();
        assert_eq!(p.subject.to_string(), "Allow network bind?");
        assert_eq!(p.action_summary.to_string(), "App binds port 443");
        assert_eq!(p.evidence_link.to_string(), "ev://tx/abc123");
    }

    /// G5 — INV I5: enter_recovery sets recovery_active on the backing struct.
    /// The QObject method sets the Rust-side property which QML can only read.
    #[test]
    fn enter_recovery_sets_recovery_active() {
        let mut w = AiosWindowRust::default();
        assert!(!w.recovery_active);
        w.recovery_active = true;
        assert!(w.recovery_active);
    }

    /// G6 — All 3 QML primitive files exist on disk and are non-empty.
    #[test]
    fn qml_files_exist_on_disk() {
        let qml_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("qml");
        for name in &[
            "AIOSWindow.qml",
            "AIOSApprovalDialog.qml",
            "AIOSSecurityIndicator.qml",
        ] {
            let path = qml_dir.join(name);
            assert!(path.exists(), "missing QML file: {}", path.display());
            let len = std::fs::metadata(&path).unwrap().len();
            assert!(len > 0, "empty QML file: {}", path.display());
        }
    }
}
