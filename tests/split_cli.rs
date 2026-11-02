use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const FIXTURE_BUDGET: &str = "8";

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create fixture copy directory");
    for entry in fs::read_dir(from).expect("read fixture directory") {
        let entry = entry.expect("read fixture entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("fixture entry type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy fixture file");
        }
    }
}

/// Copies the fixture crate to a fresh directory named `case`, splits its `engine/mod.rs`,
/// and returns the crate copy and the split's stdout.
fn split_fixture(case: &str) -> (PathBuf, String) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/split_crate");
    let copy = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    if copy.exists() {
        fs::remove_dir_all(&copy).expect("remove previous fixture copy");
    }
    copy_dir(&fixture, &copy);

    let file = copy.join("src").join("engine").join("mod.rs");
    let output = Command::new(env!("CARGO_BIN_EXE_file_splitter_rs"))
        .arg("split")
        .arg(&file)
        .args(["--budget", FIXTURE_BUDGET])
        .output()
        .expect("run file_splitter_rs split");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        output.status.success(),
        "split failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (copy, stdout)
}

#[test]
fn mod_rs_source_stays_the_root_with_parts_beside_it() {
    let (copy, _) = split_fixture("layout");
    let engine = copy.join("src").join("engine");

    for file in ["mod.rs", "child.rs", "part_0.rs", "part_1.rs", "part_2.rs"] {
        assert!(engine.join(file).is_file(), "missing engine/{file}");
    }
    assert!(!engine.join("part_3.rs").exists());
    assert!(engine.join("tests").join("mod.rs").is_file());
    assert!(engine.join("tests").join("part_0.rs").is_file());
    assert!(!engine.join("mod").exists(), "parts went to engine/mod/");

    let root = fs::read_to_string(engine.join("mod.rs")).expect("read root");
    assert!(root.contains("mod child;"));
    assert!(
        root.contains("pub fn run("),
        "pub fn left the root:\n{root}"
    );
    assert!(root.contains("mod part_0;"));
}

#[test]
fn split_prints_the_output_directory() {
    let (copy, stdout) = split_fixture("prints");
    let engine = copy.join("src").join("engine");

    let expected = format!("Output directory: {}", engine.display());
    assert!(
        stdout.contains(&expected),
        "missing `{expected}` in:\n{stdout}"
    );
    let part = engine.join("part_0.rs");
    let part_line = format!("{} (", part.display());
    assert!(
        stdout.contains(&part_line),
        "missing written file line in:\n{stdout}"
    );
}

#[test]
fn split_fixture_compiles() {
    let (copy, _) = split_fixture("compiles");
    let engine = copy.join("src").join("engine");

    let mut alpha_part = None;
    let mut beta_part = None;
    for i in 0..3 {
        let part = fs::read_to_string(engine.join(format!("part_{i}.rs"))).expect("read part");
        assert_eq!(
            part.lines().next(),
            Some("use super::*;"),
            "part_{i} header"
        );
        if part.contains("fn alpha_start") {
            alpha_part = Some(i);
        }
        if part.contains("fn beta_finish") {
            beta_part = Some(i);
        }
    }
    assert!(alpha_part.is_some() && beta_part.is_some());
    assert_ne!(
        alpha_part, beta_part,
        "the cross-part call is not exercised"
    );

    let cargo = std::env::var_os("CARGO").expect("cargo test sets CARGO");
    let output = Command::new(cargo)
        .args(["check", "--all-targets", "--quiet", "--manifest-path"])
        .arg(copy.join("Cargo.toml"))
        .output()
        .expect("run cargo check on the split fixture");
    assert!(
        output.status.success(),
        "split fixture does not compile:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
