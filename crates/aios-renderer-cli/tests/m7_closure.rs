//! T-063 M7 closure invariants.

#![allow(
    clippy::items_after_statements,
    reason = "closure tests keep filesystem setup next to the checked invariant"
)]

use std::error::Error;
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

#[test]
fn crate_version_is_exactly_0_1_0() -> TestResult {
    let cargo_toml = read_crate_file("Cargo.toml")?;

    assert!(cargo_toml.contains("version = \"0.1.0\""));
    assert!(!cargo_toml.contains("version = \"0.0.1\""));
    Ok(())
}

#[test]
fn renderer_cli_src_contains_no_todo_or_unimplemented_macros() -> TestResult {
    for file in rust_files(&crate_dir().join("src"))? {
        let contents = std::fs::read_to_string(&file)?;
        assert!(
            !contents.contains("todo!()"),
            "todo!() found in {}",
            file.display()
        );
        assert!(
            !contents.contains("unimplemented!()"),
            "unimplemented!() found in {}",
            file.display()
        );
    }
    Ok(())
}

#[test]
fn aios_binary_target_exists_for_integration_tests() {
    let binary = Path::new(env!("CARGO_BIN_EXE_aios"));

    assert!(
        binary.exists(),
        "missing aios binary at {}",
        binary.display()
    );
}

#[test]
fn every_output_format_variant_has_render_test_coverage() -> TestResult {
    let tests = all_test_sources()?;

    for variant in ["Text", "Json", "Tree", "Table"] {
        let needle = format!("OutputFormat::{variant}");
        assert!(tests.contains(&needle), "missing render test for {needle}");
    }

    Ok(())
}

#[test]
fn every_aios_subcommand_has_cli_integration_coverage() -> TestResult {
    let tests = all_test_sources()?;

    for (parent, child) in [
        ("action", "submit"),
        ("action", "status"),
        ("fs", "read"),
        ("fs", "list"),
        ("fs", "list-versions"),
        ("policy", "evaluate"),
        ("vault", "list-capabilities"),
        ("vault", "issue"),
        ("evidence", "chain"),
        ("evidence", "get"),
    ] {
        assert!(
            tests.contains(&format!("\"{parent}\"")) && tests.contains(&format!("\"{child}\"")),
            "missing CLI test for {parent} {child}"
        );
    }

    Ok(())
}

#[test]
fn renderable_impls_cover_m7_cross_crate_types() -> TestResult {
    let src = all_src_sources()?;

    for impl_header in [
        "impl Renderable for ActionContext",
        "impl Renderable for EvidenceReceipt",
        "impl Renderable for PolicyDecision",
        "impl Renderable for VaultCapability",
        "impl Renderable for Object",
    ] {
        assert!(src.contains(impl_header), "missing {impl_header}");
    }

    Ok(())
}

#[test]
fn closure_test_files_exist_for_mvp_acceptance_and_closure() {
    for relative in [
        "tests/mvp_full_golden_path.rs",
        "tests/acceptance_fixtures.rs",
        "tests/m7_closure.rs",
    ] {
        let path = crate_dir().join(relative);
        assert!(path.exists(), "missing {}", path.display());
    }
}

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_crate_file(relative: &str) -> TestResult<String> {
    Ok(std::fs::read_to_string(crate_dir().join(relative))?)
}

fn all_test_sources() -> TestResult<String> {
    read_all_rust_sources(&crate_dir().join("tests"))
}

fn all_src_sources() -> TestResult<String> {
    read_all_rust_sources(&crate_dir().join("src"))
}

fn read_all_rust_sources(root: &Path) -> TestResult<String> {
    let mut out = String::new();
    for file in rust_files(root)? {
        out.push_str(&std::fs::read_to_string(file)?);
        out.push('\n');
    }
    Ok(out)
}

fn rust_files(root: &Path) -> TestResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_rust_files(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_rust_files(root: &Path, files: &mut Vec<PathBuf>) -> TestResult {
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(())
}
