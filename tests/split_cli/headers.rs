use std::fs;

use crate::helpers::{read_all, rust_files, split_fixture};

#[test]
fn impl_attributes_are_on_every_emitted_block() {
    let (copy, _) = split_fixture("impl_attrs");
    let engine = copy.path().join("src").join("engine");
    let files = read_all(&rust_files(&engine, &["child.rs"]));

    let mut blocks = 0;
    let mut root_blocks = 0;
    for (path, text) in &files {
        let rows: Vec<&str> = text.lines().collect();
        for (at, row) in rows.iter().enumerate() {
            if *row != "impl Engine {" {
                continue;
            }
            assert!(
                at > 0 && rows[at - 1] == "#[cfg(not(any()))]",
                "`impl Engine` without its attribute in {}:\n{text}",
                path.display()
            );
            blocks += 1;
            if path.ends_with("engine/mod.rs") {
                root_blocks += 1;
            }
        }
    }
    assert_eq!(root_blocks, 1, "no partial impl Engine in the root");
    assert!(blocks >= 3, "only {blocks} impl Engine blocks");
}

#[test]
fn impl_header_comments_and_inner_attributes_are_on_every_block() {
    let (copy, _) = split_fixture("impl_header");
    let engine = copy.path().join("src").join("engine");
    let files = read_all(&rust_files(&engine, &["child.rs"]));

    let mut blocks = 0;
    for (path, text) in &files {
        let rows: Vec<&str> = text.lines().collect();
        for (at, row) in rows.iter().enumerate() {
            if *row != "impl Engine {" {
                continue;
            }
            assert!(
                at >= 3
                    && rows[at - 3..at]
                        == [
                            "#[allow(unused_mut)]",
                            "// comment: between impl Engine attributes",
                            "#[cfg(not(any()))]",
                        ],
                "`impl Engine` without its attributes and comment in {}:\n{text}",
                path.display()
            );
            assert_eq!(
                rows.get(at + 1..at + 3),
                Some(
                    &[
                        "    // comment: after impl Engine brace",
                        "    #![allow(dead_code)]"
                    ][..]
                ),
                "`impl Engine` without its body top in {}:\n{text}",
                path.display()
            );
            blocks += 1;
        }
    }
    assert!(blocks >= 3, "only {blocks} impl Engine blocks");
}

#[test]
fn generic_impl_headers_are_copied_as_written() {
    let (copy, _) = split_fixture("impl_generic_header");
    let engine = copy.path().join("src").join("engine");
    let files = read_all(&rust_files(&engine, &["child.rs"]));
    let headers = [
        "impl<T> /* x */ Wrapper<T> where T: Clone // y\n{",
        "impl<T> Wrapper<T>\nwhere\n    // comment: in where clause\n    T: Clone\n        + Default,\n{",
    ];

    let mut blocks = [0; 2];
    for (path, text) in &files {
        let text = text.replace("\r\n", "\n");
        for (at, _) in text.match_indices("impl<T>") {
            let header = headers
                .iter()
                .position(|header| text[at..].starts_with(header))
                .unwrap_or_else(|| {
                    panic!(
                        "impl<T> header not as written in {}:\n{}",
                        path.display(),
                        &text[at..]
                    )
                });
            blocks[header] += 1;
        }
    }
    assert!(
        blocks.iter().all(|&count| count >= 2),
        "each Wrapper impl is not split across blocks: {blocks:?}"
    );
}

#[test]
fn inline_module_comments_and_inner_attributes_are_kept() {
    let (copy, _) = split_fixture("inline_comments");
    let tests_dir = copy.path().join("src").join("engine").join("tests");
    let root = fs::read_to_string(tests_dir.join("mod.rs")).expect("read tests root");

    assert!(
        root.starts_with("// comment: after tests brace\n#![allow(unused_imports)]\n"),
        "the tests module file does not open with its inner attribute:\n{root}"
    );
    assert!(
        root.contains("// comment: before use super\nuse super::*;"),
        "{root}"
    );
    assert!(
        root.trim_end().ends_with("// comment: end of tests"),
        "the end-of-module comment is not at the end of the tests root:\n{root}"
    );

    let parts = read_all(&rust_files(&tests_dir, &["mod.rs"]));
    for name in ["delta_runs", "epsilon_runs"] {
        let expected =
            format!("// comment: before {name}\n#[test]\npub(super) fn {name}()");
        assert!(
            parts.iter().any(|(_, text)| text.contains(&expected)),
            "`{expected}` is not in a tests part"
        );
    }
}

#[test]
fn inline_module_declaration_keeps_visibility_and_attributes() {
    let (copy, _) = split_fixture("inline_vis");
    let root = fs::read_to_string(copy.path().join("src").join("engine").join("mod.rs"))
        .expect("read root");
    assert!(
        root.contains("#[allow(dead_code)]\npub mod shapes;"),
        "the shapes declaration lost its visibility or attribute:\n{root}"
    );
    assert!(root.contains("#[cfg(test)]\nmod tests;"));
}
