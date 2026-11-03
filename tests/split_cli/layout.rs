use std::fs;
use std::process::Command;

use crate::helpers::{read_all, rust_files, split_fixture};

#[test]
fn mod_rs_source_stays_the_root_with_parts_beside_it() {
    let (copy, _) = split_fixture("layout");
    let engine = copy.path().join("src").join("engine");

    for file in [
        "mod.rs", "child.rs", "run.rs", "emitted.rs", "emit.rs", "gamma.rs", "alpha.rs", "beta.rs", "wrap.rs", "second.rs", "zeta.rs",
        "swap.rs", "shape.rs", "omega.rs", "total.rs",
    ] {
        assert!(engine.join(file).is_file(), "missing engine/{file}");
    }
    assert!(engine.join("tests").join("mod.rs").is_file());
    assert!(engine.join("tests").join("delta.rs").is_file());
    assert!(!engine.join("mod").exists(), "parts went to engine/mod/");

    let root = fs::read_to_string(engine.join("mod.rs")).expect("read root");
    assert!(root.contains("mod child;"));
    // The root's fixed items already pass the budget of 8: its `pub` functions move and
    // the root re-exports them.
    assert!(
        root.contains("pub use run::run;") && !root.contains("pub fn run("),
        "pub fn run is not re-exported from its part:\n{root}"
    );
    assert!(root.contains("mod alpha;"));
}

#[test]
fn parts_are_named_from_their_items() {
    let (copy, stdout) = split_fixture("names");
    let engine = copy.path().join("src").join("engine");

    let files = rust_files(&copy.path(), &[]);
    assert!(
        !files.iter().any(|file| file
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("part_"))),
        "a part has no name from its items:\n{stdout}"
    );
    // `wrap_first` and `wrap_second` are in two parts: the first takes `wrap`, the second
    // its next word. `Shape for Engine` takes `shape`, never the parent's name `engine`.
    let wrap = fs::read_to_string(engine.join("wrap.rs")).expect("read wrap part");
    assert!(wrap.contains("fn wrap_first"));
    let second = fs::read_to_string(engine.join("second.rs")).expect("read second part");
    assert!(second.contains("fn wrap_second"));
    let shape = fs::read_to_string(engine.join("shape.rs")).expect("read shape part");
    assert!(shape.contains("impl Shape for Engine"));
}

#[test]
fn split_prints_the_output_directory() {
    let (copy, stdout) = split_fixture("prints");
    let engine = copy.path().join("src").join("engine");

    let expected = format!("Output directory: {}", engine.display());
    assert!(
        stdout.contains(&expected),
        "missing `{expected}` in:\n{stdout}"
    );
    let part = engine.join("alpha.rs");
    let part_line = format!("{} (", part.display());
    assert!(
        stdout.contains(&part_line),
        "missing written file line in:\n{stdout}"
    );
}

#[test]
fn test_root_split_stays_a_test_target() {
    let (copy, _) = split_fixture("test_root");
    let tests = copy.path().join("tests");

    assert!(!tests.join("flow.rs").exists(), "tests/flow.rs was kept");
    assert!(tests.join("flow").join("main.rs").is_file());
    assert!(!tests.join("flow").join("mod.rs").exists());
    // `tests/shared.rs` is a module of `tests/suite.rs`, not a target.
    assert!(!tests.join("shared.rs").exists(), "tests/shared.rs was kept");
    assert!(tests.join("shared").join("mod.rs").is_file());
    assert!(!tests.join("shared").join("main.rs").exists());

    let cargo = std::env::var_os("CARGO").expect("cargo test sets CARGO");
    for target in ["flow", "suite"] {
        let output = Command::new(&cargo)
            .args(["test", "--quiet", "--test", target, "--manifest-path"])
            .arg(copy.path().join("Cargo.toml"))
            .output()
            .expect("run cargo test on the split fixture");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "split test target {target} fails:\n{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains("2 passed"),
            "the 2 tests of {target} did not run:\n{stdout}"
        );
    }
}

#[test]
fn output_code_macro_input_keeps_its_paths() {
    let (copy, _) = split_fixture("quote");
    let files = read_all(&rust_files(&copy.path().join("src").join("engine"), &[]));

    let (path, text) = files
        .iter()
        .find(|(_, text)| text.contains("fn emit_call"))
        .expect("a file holds emit_call");
    assert!(!path.ends_with("mod.rs"), "emit_call stayed in the root");
    assert!(
        text.contains("quote!(super::systems::serve(&hand))"),
        "the quote! input changed:\n{text}"
    );
}

#[test]
fn restricted_item_used_only_in_its_file_is_not_re_exported() {
    let (copy, _) = split_fixture("reexport");
    let root = fs::read_to_string(copy.path().join("src").join("engine").join("mod.rs"))
        .expect("read root");

    assert!(
        !root.contains("pub(crate) use"),
        "omega_count is re-exported, but no other file of the crate names it:\n{root}"
    );
}

#[test]
fn split_fixture_compiles() {
    let (copy, _) = split_fixture("compiles");
    let engine = copy.path().join("src").join("engine");

    let alpha = fs::read_to_string(engine.join("alpha.rs")).expect("read alpha part");
    let beta = fs::read_to_string(engine.join("beta.rs")).expect("read beta part");
    assert!(alpha.contains("fn alpha_start") && beta.contains("fn beta_finish"));
    assert_eq!(alpha.lines().next(), Some("use super::*;"), "alpha header");

    // The split adds no unused import: no `use super::*;` that a part does not need, no
    // root import or re-export that nothing names.
    let cargo = std::env::var_os("CARGO").expect("cargo test sets CARGO");
    let output = Command::new(cargo)
        .args(["check", "--all-targets", "--quiet", "--manifest-path"])
        .arg(copy.path().join("Cargo.toml"))
        .env("RUSTFLAGS", "-D unused-imports")
        .output()
        .expect("run cargo check on the split fixture");
    assert!(
        output.status.success(),
        "split fixture does not compile:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
