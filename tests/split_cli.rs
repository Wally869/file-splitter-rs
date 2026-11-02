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

    let stdout = run_split(&copy.join("src").join("engine").join("mod.rs"));
    (copy, stdout)
}

/// Copies `tests/fixtures/mixed.rs` to a fresh directory named `case`, splits it, and
/// returns the folder of the split output.
fn split_mixed(case: &str) -> PathBuf {
    let copy = Path::new(env!("CARGO_TARGET_TMPDIR")).join(case);
    if copy.exists() {
        fs::remove_dir_all(&copy).expect("remove previous fixture copy");
    }
    fs::create_dir_all(&copy).expect("create fixture copy directory");
    let file = copy.join("mixed.rs");
    fs::copy(mixed_fixture(), &file).expect("copy mixed fixture");
    run_split(&file);
    copy.join("mixed")
}

fn mixed_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mixed.rs")
}

fn run_split(file: &Path) -> String {
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
    stdout
}

/// Every `.rs` file under `dir`, except `skip`.
fn rust_files(dir: &Path, skip: &[&str]) -> Vec<PathBuf> {
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

fn read_all(files: &[PathBuf]) -> Vec<(PathBuf, String)> {
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

/// Sorted keys of the items in `files`: kind and name, with the impl target for impl items
/// and trait impls. `use` lines and `mod part_N` declarations are left out: the split adds
/// them.
fn item_keys(files: &[PathBuf]) -> Vec<String> {
    let mut keys = Vec::new();
    for (file, text) in read_all(files) {
        let parsed = syn::parse_file(&text)
            .unwrap_or_else(|e| panic!("{} does not parse: {e}", file.display()));
        collect_keys(&parsed.items, &mut keys);
    }
    keys.sort();
    keys
}

fn collect_keys(items: &[syn::Item], keys: &mut Vec<String>) {
    use quote::ToTokens;
    use syn::{ImplItem, Item};
    let text = |node: &dyn ToTokens| node.to_token_stream().to_string();
    for item in items {
        let key = match item {
            Item::Fn(f) => format!("fn {}", f.sig.ident),
            Item::Impl(i) => {
                let target = match &i.trait_ {
                    Some((_, path, _)) => format!("{} for {}", text(path), text(&i.self_ty)),
                    None => text(&i.self_ty),
                };
                for impl_item in &i.items {
                    let name = match impl_item {
                        ImplItem::Fn(f) => format!("fn {}", f.sig.ident),
                        ImplItem::Const(c) => format!("const {}", c.ident),
                        ImplItem::Type(t) => format!("type {}", t.ident),
                        other => format!("other {}", text(other)),
                    };
                    keys.push(format!("impl {target}: {name}"));
                }
                if i.trait_.is_none() {
                    continue;
                }
                format!("impl {target}")
            }
            Item::Mod(m) => {
                if m.ident.to_string().starts_with("part_") {
                    continue;
                }
                if let Some((_, inner)) = &m.content {
                    collect_keys(inner, keys);
                }
                format!("mod {}", m.ident)
            }
            Item::Use(_) => continue,
            Item::Struct(s) => format!("struct {}", s.ident),
            Item::Enum(e) => format!("enum {}", e.ident),
            Item::Union(u) => format!("union {}", u.ident),
            Item::Trait(t) => format!("trait {}", t.ident),
            Item::Const(c) => format!("const {}", c.ident),
            Item::Static(s) => format!("static {}", s.ident),
            Item::Type(t) => format!("type {}", t.ident),
            Item::ExternCrate(e) => format!("extern crate {}", e.ident),
            Item::ForeignMod(f) => format!("extern {}", text(f)),
            Item::Macro(m) => format!("macro {}", text(m)),
            other => format!("other {}", text(other)),
        };
        keys.push(key);
    }
}

#[test]
fn output_keeps_every_item_of_the_compile_fixture() {
    let (copy, _) = split_fixture("items_compile");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/split_crate");
    let input = item_keys(&[fixture.join("src").join("engine").join("mod.rs")]);
    let output = item_keys(&rust_files(&copy.join("src").join("engine"), &["child.rs"]));
    assert_eq!(input, output);
}

#[test]
fn output_keeps_every_item_of_the_mixed_fixture() {
    let out = split_mixed("items_mixed");
    let input = item_keys(&[mixed_fixture()]);
    let output = item_keys(&rust_files(&out, &[]));
    assert_eq!(input, output);
}

#[test]
fn trait_impl_moves_as_one_block() {
    let (copy, _) = split_fixture("trait_impl");
    let engine = copy.join("src").join("engine");
    let files = read_all(&rust_files(&engine, &["child.rs"]));

    let holders: Vec<&(PathBuf, String)> = files
        .iter()
        .filter(|(_, text)| text.contains("impl Shape for Engine"))
        .collect();
    assert_eq!(holders.len(), 1, "impl Shape for Engine is not in one file");
    let (path, text) = holders[0];
    assert_eq!(text.matches("impl Shape for Engine").count(), 1);
    assert!(
        !path.ends_with("mod.rs"),
        "the trait impl stayed in the root"
    );
    for member in [
        "type Output = u32;",
        "const SIDES: u32 = 4;",
        "fn area(&self) -> u32 {",
        "fn sides(&self) -> u32 {",
    ] {
        let count: usize = files
            .iter()
            .map(|(_, text)| text.matches(member).count())
            .sum();
        assert_eq!(count, 1, "`{member}` appears {count} times");
        assert!(text.contains(member), "`{member}` left the trait impl");
    }
}

#[test]
fn items_the_split_does_not_move_stay_in_the_root() {
    let out = split_mixed("unmoved");
    let root = fs::read_to_string(out.join("mod.rs")).expect("read root");
    for item in [
        "extern crate alloc;",
        "pub union Word {",
        "unsafe extern \"C\" {",
        "macro_rules! square {",
        "static LIMIT: u32 = 10;",
        "const SCALE: u32 = 3;",
        "type Size = u32;",
        "const MARGIN: u32 = 2;",
    ] {
        assert!(
            root.contains(item),
            "`{item}` is missing from the root:\n{root}"
        );
    }
}

#[test]
fn moved_code_keeps_literal_text() {
    let out = split_mixed("literals");
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

/// The line before the first line of `files` that equals `line`, with that file's text.
fn line_before<'a>(files: &'a [(PathBuf, String)], line: &str) -> (&'a PathBuf, &'a str) {
    for (path, text) in files {
        let rows: Vec<&str> = text.lines().collect();
        if let Some(at) = rows.iter().position(|row| *row == line) {
            assert!(at > 0, "`{line}` is the first line of {}", path.display());
            return (path, rows[at - 1]);
        }
    }
    panic!("no output file has the line `{line}`");
}

#[test]
fn comments_stay_next_to_their_items() {
    let out = split_mixed("comments");
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
fn impl_attributes_are_on_every_emitted_block() {
    let (copy, _) = split_fixture("impl_attrs");
    let engine = copy.join("src").join("engine");
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
fn inline_module_declaration_keeps_visibility_and_attributes() {
    let (copy, _) = split_fixture("inline_vis");
    let root =
        fs::read_to_string(copy.join("src").join("engine").join("mod.rs")).expect("read root");
    assert!(
        root.contains("#[allow(dead_code)]\npub mod shapes;"),
        "the shapes declaration lost its visibility or attribute:\n{root}"
    );
    assert!(root.contains("#[cfg(test)]\nmod tests;"));
}

#[test]
fn super_rewrite_skips_literals_and_comments() {
    let out = split_mixed("super_paths");
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

#[test]
fn mod_rs_source_stays_the_root_with_parts_beside_it() {
    let (copy, _) = split_fixture("layout");
    let engine = copy.join("src").join("engine");

    for file in [
        "mod.rs",
        "child.rs",
        "part_0.rs",
        "part_1.rs",
        "part_2.rs",
        "part_3.rs",
    ] {
        assert!(engine.join(file).is_file(), "missing engine/{file}");
    }
    assert!(!engine.join("part_4.rs").exists());
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
    for i in 0..4 {
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
