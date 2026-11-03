use std::fs;
use std::path::PathBuf;

use crate::helpers::{
    compile_fixture, mixed_fixture, read_all, rust_files, split_fixture, split_mixed,
};

/// The item keys of a split output, without the `mod` lines of the parts the split added:
/// every `mod` key that `input` does not have.
fn output_keys(files: &[PathBuf], input: &[String]) -> Vec<String> {
    item_keys(files)
        .into_iter()
        .filter(|key| !key.starts_with("mod ") || input.contains(key))
        .collect()
}

/// Sorted keys of the items in `files`: kind and name, with the impl target for impl items
/// and trait impls. `use` lines are left out: the split adds them.
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
    let input = item_keys(&[compile_fixture().join("src").join("engine").join("mod.rs")]);
    let output = output_keys(
        &rust_files(&copy.path().join("src").join("engine"), &["child.rs"]),
        &input,
    );
    assert_eq!(input, output);

    let tests = compile_fixture().join("tests");
    let input = item_keys(&[
        tests.join("flow.rs"),
        tests.join("suite.rs"),
        tests.join("shared.rs"),
    ]);
    let output = output_keys(&rust_files(&copy.path().join("tests"), &[]), &input);
    assert_eq!(input, output);
}

#[test]
fn output_keeps_every_item_of_the_mixed_fixture() {
    let (_scratch, out) = split_mixed("items_mixed");
    let input = item_keys(&[mixed_fixture()]);
    let output = output_keys(&rust_files(&out, &[]), &input);
    assert_eq!(input, output);
}

#[test]
fn trait_impl_moves_as_one_block() {
    let (copy, _) = split_fixture("trait_impl");
    let engine = copy.path().join("src").join("engine");
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
    let (_scratch, out) = split_mixed("unmoved");
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
