//! SBOM / provenance module for AIOS supply-chain evidence.
#![allow(clippy::doc_markdown, clippy::missing_const_for_fn)]

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SbomFormat { Spdx, CycloneDx }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbomComponent {
    pub name: String,
    pub version: String,
    pub supplier: String,
    pub sha256: String,
}

impl SbomComponent {
    pub fn new(name: String, version: String, supplier: String, sha256: String) -> Self {
        Self { name, version, supplier, sha256 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbomDocument {
    pub components: Vec<SbomComponent>,
    pub format: SbomFormat,
    pub generated_at: u64,
    pub signature: Option<Vec<u8>>,
}

impl SbomDocument {
    pub fn new(components: Vec<SbomComponent>, format: SbomFormat, generated_at: u64) -> Self {
        Self { components, format, generated_at, signature: None }
    }

    pub fn sign(&mut self, signature: Vec<u8>) { self.signature = Some(signature); }

    pub fn verify_signature(&self, expected: &[u8]) -> bool {
        match &self.signature { Some(sig) => sig == expected, None => false }
    }

    pub fn component_count(&self) -> usize { self.components.len() }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlsaProvenance {
    pub builder_id: String,
    pub source_repo: String,
    pub build_command: String,
    pub output_hashes: HashMap<String, String>,
}

impl SlsaProvenance {
    pub fn new(builder_id: String, source_repo: String, build_command: String) -> Self {
        Self { builder_id, source_repo, build_command, output_hashes: HashMap::new() }
    }

    pub fn add_output_hash(&mut self, filename: String, hash: String) {
        self.output_hashes.insert(filename, hash);
    }

    pub fn verify_builder(&self, expected_builder: &str) -> bool {
        self.builder_id == expected_builder
    }

    pub fn verify_output(&self, filename: &str, expected_hash: &str) -> bool {
        self.output_hashes.get(filename).map_or(false, |h| h == expected_hash)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VexStatus { Affected, NotAffected, Fixed }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VexStatement {
    pub vulnerability_id: String,
    pub component_name: String,
    pub status: VexStatus,
    pub justification: String,
}

impl VexStatement {
    pub fn new(vuln_id: String, component: String, status: VexStatus, justification: String) -> Self {
        Self { vulnerability_id: vuln_id, component_name: component, status, justification }
    }

    pub fn is_fixed(&self) -> bool { self.status == VexStatus::Fixed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn sbom_generation() {
        let components = vec![
            SbomComponent::new("libc".into(), "2.35".into(), "GNU".into(), "abc123".into()),
        ];
        let doc = SbomDocument::new(components, SbomFormat::Spdx, 1000);
        assert_eq!(doc.component_count(), 1);
        assert_eq!(doc.format, SbomFormat::Spdx);
    }

    #[test] fn sbom_signature_verification() {
        let mut doc = SbomDocument::new(vec![], SbomFormat::CycloneDx, 1000);
        assert!(!doc.verify_signature(b"sig"));
        doc.sign(b"sig".to_vec());
        assert!(doc.verify_signature(b"sig"));
        assert!(!doc.verify_signature(b"wrong"));
    }

    #[test] fn slsa_builder_verification() {
        let provenance = SlsaProvenance::new("github-actions".into(), "repo".into(), "make".into());
        assert!(provenance.verify_builder("github-actions"));
        assert!(!provenance.verify_builder("other"));
    }

    #[test] fn slsa_output_hash_verification() {
        let mut p = SlsaProvenance::new("b".into(), "r".into(), "c".into());
        p.add_output_hash("binary".into(), "hash123".into());
        assert!(p.verify_output("binary", "hash123"));
        assert!(!p.verify_output("binary", "wrong"));
        assert!(!p.verify_output("missing", "hash"));
    }

    #[test] fn vex_status_detection() {
        let fixed = VexStatement::new("CVE-2024-0001".into(), "libc".into(), VexStatus::Fixed, "patched".into());
        assert!(fixed.is_fixed());
        let affected = VexStatement::new("CVE-2024-0002".into(), "openssl".into(), VexStatus::Affected, "pending".into());
        assert!(!affected.is_fixed());
    }

    #[test] fn sbom_multiple_components() {
        let comps: Vec<_> = (0..3).map(|i| SbomComponent::new(format!("pkg{i}"), "1.0".into(), "test".into(), "h".into())).collect();
        let doc = SbomDocument::new(comps, SbomFormat::Spdx, 1000);
        assert_eq!(doc.component_count(), 3);
    }

    #[test] fn sbom_format_enum() {
        assert_ne!(SbomFormat::Spdx, SbomFormat::CycloneDx);
    }
}
