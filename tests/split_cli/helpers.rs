use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

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

/// A folder of its own for one test run, removed when the returned `TempDir` drops.
fn scratch(case: &str) -> TempDir {
    tempfile::Builder::new()
        .prefix(&format!("{case}-"))
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .expect("create scratch folder")
}

pub fn compile_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/split_crate")
}

/// Copies the fixture crate to a fresh scratch folder named after `case`, splits its
/// `engine/mod.rs`, and returns the scratch folder (the crate copy) and the split's stdout.
pub fn split_fixture(case: &str) -> (TempDir, String) {
    let copy = scratch(case);
    copy_dir(&compile_fixture(), copy.path());

    let stdout = run_split(&copy.path().join("src").join("engine").join("mod.rs"));
    (copy, stdout)
}

/// Copies `tests/fixtures/mixed.rs` to a fresh scratch folder named after `case`, splits
/// it, and returns the scratch folder and the folder of the split output inside it.
pub fn split_mixed(case: &str) -> (TempDir, PathBuf) {
    let copy = scratch(case);
    let file = copy.path().join("mixed.rs");
    fs::copy(mixed_fixture(), &file).expect("copy mixed fixture");
    run_split(&file);
    let out = copy.path().join("mixed");
    (copy, out)
}

pub fn mixed_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mixed.rs")
}

fn run_split(file: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_file_splitter_rs"))
        .arg("split")
        .arg(file)
        .args(["--budget", FIXTURE_BUDGET])
        .output()
        .expect("run file_splitter_rs split");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        output.status.success(),
        "split failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

/// Every `.rs` file under `dir`, except `skip`.
pub fn rust_files(dir: &Path, skip: &[&str]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).expect("read output directory") {
        let path = entry.expect("read output entry").path();
        if path.is_dir() {
            files.extend(rust_files(&path, skip));
        } else if path.extension().is_some_and(|ext| ext == "rs")
            && !skip.iter().any(|name| path.ends_with(name))
        {
            files.push(path);
        }
    }
    files
}

pub fn read_all(files: &[PathBuf]) -> Vec<(PathBuf, String)> {
    files
        .iter()
        .map(|file| {
            (
                file.clone(),
                fs::read_to_string(file).expect("read output file"),
            )
        })
        .collect()
}

/// The line before the first line of `files` that equals `line`, with that file's text.
pub fn line_before<'a>(files: &'a [(PathBuf, String)], line: &str) -> (&'a PathBuf, &'a str) {
    for (path, text) in files {
        let rows: Vec<&str> = text.lines().collect();
        if let Some(at) = rows.iter().position(|row| *row == line) {
            assert!(at > 0, "`{line}` is the first line of {}", path.display());
            return (path, rows[at - 1]);
        }
    }
    panic!("no output file has the line `{line}`");
}
