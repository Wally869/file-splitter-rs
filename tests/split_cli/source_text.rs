use std::fs;

use crate::helpers::{line_before, read_all, rust_files, split_mixed};

#[test]
fn moved_code_keeps_literal_text() {
    let (_scratch, out) = split_mixed("literals");
    let parts = read_all(&rust_files(&out, &["mod.rs"]));
    for label in ["top fn", "top method", "inner fn", "inner method"] {
        let multi_line = format!("\"{label}\n  second line\n            third line\"");
        let (first, rest) = label.split_once(' ').expect("two-word label");
        let raw = format!("r#\"{first} \"raw\" {rest}\n      indented\nend\"#");
        for literal in [multi_line, raw] {
            assert!(
                parts.iter().any(|(_, text)| text.contains(&literal)),
                "the parts lost or changed the literal {literal}"
            );
        }
    }
}

#[test]
fn comments_stay_next_to_their_items() {
    let (_scratch, out) = split_mixed("comments");
    let files = read_all(&rust_files(&out, &[]));

    let (path, before) = line_before(
        &files,
        "pub(in super::super) fn free_banner() -> &'static str {",
    );
    assert_eq!(before, "// comment: before free_banner");
    assert!(!path.ends_with("mod.rs"), "free_banner stayed in the root");

    let (path, before) = line_before(
        &files,
        "    pub(in super::super) fn panel_raw(&self) -> &'static str {",
    );
    assert_eq!(before, "    // comment: before panel_raw");
    assert!(!path.ends_with("mod.rs"), "panel_raw stayed in the root");

    let root_path = out.join("mod.rs");
    let root = fs::read_to_string(&root_path).expect("read root");
    let root_only = [(root_path, root.clone())];
    let (_, before) = line_before(&root_only, "const SCALE: u32 = 3;");
    assert_eq!(before, "// comment: between LIMIT and SCALE");
    assert!(
        root.trim_end().ends_with("// comment: end of file"),
        "the end-of-file comment is not at the end of the root:\n{root}"
    );
}

#[test]
fn super_rewrite_skips_literals_and_comments() {
    let (_scratch, out) = split_mixed("super_paths");
    let files = read_all(&rust_files(&out, &["mod.rs"]));
    let (_, text) = files
        .iter()
        .find(|(_, text)| text.contains("fn path_text() -> String {"))
        .expect("path_text is in a part");
    for kept in [
        "    // super::x stays as written",
        "\"super::x\"",
        "r\"super::\"",
        "let real = super::super::Outer::NAME;",
    ] {
        assert!(text.contains(kept), "`{kept}` is not in the part:\n{text}");
    }
    assert_eq!(text.matches("super::super::").count(), 1, "{text}");
}
