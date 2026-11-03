use std::collections::HashSet;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};
use regex::Regex;

use super::cluster::{self, Cluster};
use super::names;
use super::parse::{ClassifiedFile, ImplContext, InlineModule, SplittableItem, indent_rows};

/// A file the split wrote, with its line count.
pub struct Written {
    pub path: PathBuf,
    pub lines: usize,
}

/// Where the split went and what it wrote.
pub struct Emitted {
    pub dir: PathBuf,
    pub files: Vec<Written>,
}

/// The folder of a crate: the nearest ancestor of `file` with a `Cargo.toml`.
fn crate_dir(file: &Path) -> Option<&Path> {
    file.ancestors()
        .skip(1)
        .find(|dir| dir.join("Cargo.toml").is_file())
}

/// A `.rs` file beside `file` declares `mod <stem>;`: `file` is a module of that file's
/// crate (`tests/suite.rs` with `mod tasks;`), not a crate root of its own.
fn declared_beside(file: &Path, stem: &str) -> bool {
    let Some(dir) = file.parent() else {
        return false;
    };
    let declaration = Regex::new(&format!(r"\bmod\s+{}\s*;", regex::escape(stem)))
        .expect("an escaped identifier is a valid regex");
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        path != file
            && path.extension().is_some_and(|ext| ext == "rs")
            && fs::read_to_string(&path).is_ok_and(|text| declaration.is_match(&text))
    })
}

/// Where the root and the parts of a split go: `(module dir, root path, keep the original)`.
/// - `<dir>/mod.rs`, `src/lib.rs` and `src/main.rs` stay the root; parts go beside them.
/// - A crate root target `tests/foo.rs`, `benches/foo.rs`, `examples/foo.rs` or
///   `src/bin/foo.rs` becomes `<dir>/foo/main.rs`, which cargo still finds as that target.
///   A file of `tests/`, `benches/` or `examples/` that a file beside it declares as a
///   module is not a target: it takes the rule below.
/// - Any other `<dir>/foo.rs` becomes `<dir>/foo/mod.rs`.
fn layout(original_file: &Path) -> Result<(PathBuf, PathBuf, bool)> {
    let parent = original_file
        .parent()
        .context("File has no parent directory")?;
    let name = original_file
        .file_name()
        .and_then(|name| name.to_str())
        .context("File name is not valid UTF-8")?;
    let parent_name = parent.file_name().and_then(|name| name.to_str());
    let grandparent = parent.parent();
    let parent_is_src = parent_name == Some("src")
        && grandparent.is_some_and(|dir| dir.join("Cargo.toml").is_file());
    if name == "mod.rs" || (parent_is_src && (name == "lib.rs" || name == "main.rs")) {
        return Ok((parent.to_path_buf(), original_file.to_path_buf(), true));
    }

    let file_stem = original_file
        .file_stem()
        .context("File has no stem")?
        .to_str()
        .context("File stem is not valid UTF-8")?;
    let module_dir = parent.join(file_stem);
    let target_dir = matches!(parent_name, Some("tests" | "benches" | "examples"))
        && grandparent.is_some_and(|dir| dir.join("Cargo.toml").is_file())
        && !declared_beside(original_file, file_stem);
    let bin_dir = parent_name == Some("bin")
        && grandparent.is_some_and(|dir| {
            dir.file_name().is_some_and(|n| n == "src")
                && dir.parent().is_some_and(|d| d.join("Cargo.toml").is_file())
        });
    let root_name = if target_dir || bin_dir { "main.rs" } else { "mod.rs" };
    let root_path = module_dir.join(root_name);
    Ok((module_dir, root_path, false))
}

/// The identifiers of every `.rs` file of the crate of `file`, except `file` itself and the
/// `target` folders.
fn crate_idents(file: &Path) -> HashSet<String> {
    let mut found = HashSet::new();
    let Some(dir) = crate_dir(file) else {
        return found;
    };
    let walker = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_entry(|entry| entry.file_name() != "target" && entry.file_name() != ".git");
    for entry in walker.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") || path == file {
            continue;
        }
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        if let Ok(tokens) = text.parse::<TokenStream>() {
            names::idents(tokens, &mut found);
        }
    }
    found
}

/// Emit the split files. See [`layout`] for where they go; a moved source file is deleted.
pub fn emit(
    original_file: &Path,
    classified: &ClassifiedFile,
    kept: &[SplittableItem],
    clusters: &[Cluster],
    budget: usize,
    source_idents: &HashSet<String>,
) -> Result<Emitted> {
    let (module_dir, root_path, keep_original) = layout(original_file)?;
    let context = NameContext {
        source_idents,
        crate_idents: crate_idents(original_file),
    };

    let mut head = Vec::new();
    if !classified.preamble.trim().is_empty() {
        head.push(classified.preamble.trim_end().to_string());
    }
    head.extend(
        classified
            .mod_items
            .iter()
            .map(|item| item.source_text.clone()),
    );
    head.extend(kept.iter().map(|item| item.source_text.clone()));

    let mut tail = Vec::new();
    for inline_mod in &classified.inline_modules {
        tail.push(inline_mod.declaration.clone());
    }
    if let Some(epilogue) = &classified.epilogue {
        if !tail.is_empty() {
            tail.push(String::new());
        }
        tail.push(epilogue.clone());
    }

    let mut planned = Vec::new();
    let names = plan_module(
        &module_dir,
        root_path.clone(),
        &head,
        clusters,
        &tail,
        &context,
        None,
        &mut planned,
    )?;
    let outer_head = head.join("\n\n");
    for (name, c) in names.iter().zip(clusters) {
        let items: Vec<&str> = c.items.iter().map(|item| item.name.as_str()).collect();
        println!(
            "  {name}: {} items, {} lines — {:?}",
            c.items.len(),
            c.total_lines,
            items
        );
    }
    for inline_mod in &classified.inline_modules {
        plan_inline_module(
            &module_dir,
            inline_mod,
            budget,
            &context,
            &outer_head,
            &mut planned,
        )?;
    }

    for (path, _) in &planned {
        if *path != root_path && path.exists() {
            anyhow::bail!("{} already exists; nothing was written", path.display());
        }
    }

    let mut files = Vec::new();
    for (path, content) in planned {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .with_context(|| format!("Failed to create directory {}", dir.display()))?;
        }
        fs::write(&path, &content)
            .with_context(|| format!("Failed to write {}", path.display()))?;
        files.push(Written {
            path,
            lines: content.lines().count(),
        });
    }

    if !keep_original {
        fs::remove_file(original_file)
            .with_context(|| format!("Failed to delete {}", original_file.display()))?;
    }

    Ok(Emitted {
        dir: module_dir,
        files,
    })
}

/// What part names and root imports are checked against.
struct NameContext<'a> {
    /// The names of the split file that a part module would clash with.
    source_idents: &'a HashSet<String>,
    /// The identifiers of the other files of its crate.
    crate_idents: HashSet<String>,
}

/// Plan an inline module (e.g. `#[cfg(test)] mod tests { ... }`) as a subdirectory with its
/// own mod.rs and part files. It stays a child of the same module, so its own `use`
/// lines and items keep their paths.
fn plan_inline_module(
    parent_dir: &Path,
    inline_mod: &InlineModule,
    budget: usize,
    context: &NameContext,
    outer_head: &str,
    planned: &mut Vec<(PathBuf, String)>,
) -> Result<()> {
    let mod_dir = parent_dir.join(&inline_mod.name);
    let clusters = cluster::cluster_items(&inline_mod.splittable_items, budget);

    let mut head: Vec<String> = inline_mod.body_top.iter().cloned().collect();
    head.extend(inline_mod.use_statements.iter().cloned());
    head.extend(
        inline_mod
            .mod_items
            .iter()
            .map(|item| item.source_text.clone()),
    );
    let tail: Vec<String> = inline_mod.epilogue.iter().cloned().collect();
    let root_path = mod_dir.join("mod.rs");
    let names = plan_module(
        &mod_dir,
        root_path,
        &head,
        &clusters,
        &tail,
        context,
        Some(outer_head),
        planned,
    )?;

    println!(
        "  Inline module '{}': {} clusters",
        inline_mod.name,
        clusters.len()
    );
    for (name, c) in names.iter().zip(&clusters) {
        let items: Vec<&str> = c.items.iter().map(|item| item.name.as_str()).collect();
        println!("    {name}: {} items — {:?}", c.items.len(), items);
    }
    Ok(())
}

/// Plan a module root and its parts, and return the part names. The root is `head`, the
/// part declarations with the imports of moved free functions, then `tail`.
fn plan_module(
    dir: &Path,
    root_path: PathBuf,
    head: &[String],
    clusters: &[Cluster],
    tail: &[String],
    context: &NameContext,
    outer_head: Option<&str>,
    planned: &mut Vec<(PathBuf, String)>,
) -> Result<Vec<String>> {
    let bodies: Vec<String> = clusters.iter().map(render_part).collect::<Result<_>>()?;
    let names = names::part_names(clusters, dir, context.source_idents);
    let head_text = head.join("\n\n");
    let imports = part_imports(&head_text, clusters, &bodies, &names, &context.crate_idents);

    // What `use super::*;` brings into a part: the root's own text with its new imports,
    // and for an inline module the text of the module around it (its `use super::*;`).
    let own_text = format!("{head_text}\n{}\n{}", imports.join("\n"), tail.join("\n"));
    let mut parent_text = own_text.clone();
    if let Some(outer) = outer_head {
        parent_text.push('\n');
        parent_text.push_str(outer);
    }
    let parts: Vec<String> = bodies
        .into_iter()
        .map(|body| format!("{}{body}", super_glob(&body, &own_text, &parent_text)))
        .collect();

    let mut lines = Vec::new();
    if !head.is_empty() {
        lines.push(head_text.clone());
        lines.push(String::new());
    }
    for name in &names {
        lines.push(format!("mod {name};"));
    }
    if !clusters.is_empty() {
        lines.push(String::new());
    }
    if !imports.is_empty() {
        lines.extend(imports);
        lines.push(String::new());
    }
    for line in tail {
        lines.push(line.clone());
    }
    if !tail.is_empty() {
        lines.push(String::new());
    }

    planned.push((root_path, lines.join("\n")));
    for (name, content) in names.iter().zip(parts) {
        planned.push((dir.join(format!("{name}.rs")), content));
    }
    Ok(names)
}

/// Names a part never gets from its parent: keywords, primitive types, the roots of the
/// standard library and the names of the standard prelude.
const NOT_FROM_PARENT: &[&str] = &[
    "self", "Self", "super", "crate", "std", "core", "alloc", "bool", "char", "str", "u8",
    "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize", "f32",
    "f64", "Option", "Some", "None", "Result", "Ok", "Err", "Vec", "String", "Box", "ToString",
    "ToOwned", "Clone", "Copy", "Default", "Debug", "Drop", "Eq", "PartialEq", "Ord",
    "PartialOrd", "Hash", "Iterator", "IntoIterator", "Extend", "FromIterator", "From", "Into",
    "TryFrom", "TryInto", "AsRef", "AsMut", "Fn", "FnMut", "FnOnce", "Send", "Sync", "Sized",
    "Unpin", "derive", "test", "cfg", "allow", "doc", "inline", "must_use", "let", "mut", "fn",
    "pub", "impl", "for", "in", "if", "else", "match", "return", "where", "as", "ref", "true",
    "false", "while", "loop", "break", "continue", "const", "static", "struct", "enum", "type",
    "use", "mod", "trait", "unsafe", "move", "dyn", "async", "await",
];

/// The first line of a part:
/// - `use super::*;` when the part names something its parent declares or imports;
/// - `#[allow(unused_imports)] use super::*;` when it names nothing the parent text shows,
///   but may still need the parent: the parent has a glob import (its names are unknown),
///   or the part calls a method and the parent imports or declares a trait;
/// - nothing in the other cases.
fn super_glob(body: &str, own_text: &str, parent_text: &str) -> &'static str {
    // Every identifier of the parent: a part may need any of them. Text that does not
    // tokenize keeps the line.
    let mut parent_names = HashSet::new();
    // A prelude name counts when the parent declares its own (`type Result<T> = ..`).
    let mut declared = HashSet::new();
    let Ok(tokens) = parent_text.parse::<TokenStream>() else {
        return "use super::*;\n\n";
    };
    names::idents(tokens.clone(), &mut parent_names);
    names::module_clashes(tokens.clone(), &mut declared);
    names::value_declarations(tokens.clone(), &mut declared);
    // A `macro_rules!` macro reaches the parts by its place in the text, not by the glob.
    for name in names::macro_rules_names(tokens) {
        if !declared.contains(&name) {
            parent_names.remove(&name);
        }
    }
    for root in ["std", "core", "alloc", "crate", "self", "super", "Self"] {
        declared.remove(root);
    }
    let names_parent = used_names(body, true).iter().any(|name| {
        parent_names.contains(name)
            && (!NOT_FROM_PARENT.contains(&name.as_str()) || declared.contains(name))
    });
    let glob = Regex::new(r"(?m)^\s*(pub(\([^)]*\))?\s+)?use\s[^;]*\*")
        .expect("a valid regex")
        .is_match(own_text);
    // A method call may need a trait the parent imports or declares, with no name of it in
    // the part.
    let trait_scope = Regex::new(r"(?m)^\s*(pub(\([^)]*\))?\s+)?(use|trait|unsafe\s+trait)\s")
        .expect("a valid regex")
        .is_match(parent_text);
    let method_call = Regex::new(r"\.\s*[A-Za-z_][A-Za-z0-9_]*\s*(::\s*<[^>]*>\s*)?\(")
        .expect("a valid regex")
        .is_match(body);
    if names_parent {
        "use super::*;\n\n"
    } else if glob || (trait_scope && method_call) {
        "#[allow(unused_imports)]\nuse super::*;\n\n"
    } else {
        ""
    }
}

/// The names `text` uses as paths or calls: identifier tokens, except those after `.` (a
/// method or a field of another value), after `fn` (a definition), and path segments that
/// are not the item (`std::path::Path` uses `Path`, not `std` or `path`); with `roots`, the
/// first segment of a path counts (`Engine::new` uses `Engine`). Comments and
/// literals do not count. Text that does not tokenize counts every word.
fn used_names(text: &str, roots: bool) -> HashSet<String> {
    fn walk(tokens: TokenStream, roots: bool, out: &mut HashSet<String>) {
        let trees: Vec<TokenTree> = tokens.into_iter().collect();
        for (at, tree) in trees.iter().enumerate() {
            match tree {
                TokenTree::Group(group) => walk(group.stream(), roots, out),
                TokenTree::Ident(ident) => {
                    let after_dot = at > 0
                        && matches!(&trees[at - 1], TokenTree::Punct(p) if p.as_char() == '.');
                    let after_fn =
                        at > 0 && matches!(&trees[at - 1], TokenTree::Ident(i) if i == "fn");
                    // `std::path::Path`: `path` is a module of another path, not the item.
                    let inner_segment = at >= 3
                        && is_path_sep(&trees, at - 2)
                        && !matches!(&trees[at - 3], TokenTree::Ident(i) if i == "self");
                    let module_segment = !roots
                        && is_path_sep(&trees, at + 1)
                        && !matches!(trees.get(at + 3), Some(TokenTree::Punct(p)) if p.as_char() == '<');
                    if !after_dot && !after_fn && !inner_segment && !module_segment {
                        out.insert(ident.to_string().trim_start_matches("r#").to_string());
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = HashSet::new();
    match text.parse::<TokenStream>() {
        Ok(tokens) => walk(tokens, roots, &mut out),
        Err(_) => {
            let word = Regex::new(r"[A-Za-z_][A-Za-z0-9_]*").expect("a valid regex");
            out.extend(word.find_iter(text).map(|m| m.as_str().to_string()));
        }
    }
    out
}

/// The root's `use` lines for moved free functions. A `pub` item keeps its visibility as a
/// re-export. A `pub(crate)`, `pub(super)` or `pub(in ..)` item is re-exported only when
/// another file of the crate names it; else it is handled as a private item. A private item
/// is imported when the root or another part names it, so the other parts reach it through
/// `use super::*;`. No import is written that nothing uses.
fn part_imports(
    head_text: &str,
    clusters: &[Cluster],
    parts: &[String],
    names: &[String],
    crate_idents: &HashSet<String>,
) -> Vec<String> {
    let head_names = used_names(head_text, false);
    let part_names: Vec<HashSet<String>> = parts.iter().map(|part| used_names(part, false)).collect();
    let mut imports = Vec::new();
    for ((i, cluster), part) in clusters.iter().enumerate().zip(names) {
        for item in cluster
            .items
            .iter()
            .filter(|item| item.impl_context.is_none() && !item.trait_impl)
        {
            let name = &item.name;
            let used_in_crate = item.root_vis == "pub" || crate_idents.contains(name);
            if !item.root_vis.is_empty() && used_in_crate {
                imports.push(format!("{} use {part}::{name};", item.root_vis));
                continue;
            }
            let named_outside = head_names.contains(name)
                || part_names
                    .iter()
                    .enumerate()
                    .any(|(j, used)| j != i && used.contains(name));
            if named_outside {
                imports.push(format!("use {part}::{name};"));
            }
        }
    }
    imports
}

fn is_path_sep(trees: &[TokenTree], at: usize) -> bool {
    matches!(
        (trees.get(at), trees.get(at + 1)),
        (Some(TokenTree::Punct(first)), Some(TokenTree::Punct(second)))
            if first.as_char() == ':' && first.spacing() == Spacing::Joint && second.as_char() == ':'
    )
}

fn is_super(trees: &[TokenTree], at: usize) -> bool {
    matches!(trees.get(at), Some(TokenTree::Ident(ident)) if ident == "super")
}

/// `(super)` or `(in super)` after `pub`.
fn is_super_restriction(trees: &[TokenTree], at: usize) -> bool {
    let Some(TokenTree::Group(group)) = trees.get(at) else {
        return false;
    };
    let after_pub = at > 0 && matches!(&trees[at - 1], TokenTree::Ident(ident) if ident == "pub");
    let inner: Vec<TokenTree> = group.stream().into_iter().collect();
    let restriction = match inner.as_slice() {
        [TokenTree::Ident(only)] => only == "super",
        [TokenTree::Ident(word), TokenTree::Ident(only)] => word == "in" && only == "super",
        _ => false,
    };
    after_pub && group.delimiter() == Delimiter::Parenthesis && restriction
}

/// Macros whose input is code they output, not code of the module that calls them: a path
/// inside them is resolved where the output goes.
const OUTPUT_CODE_MACROS: &[&str] = &[
    "quote",
    "quote_spanned",
    "parse_quote",
    "parse_quote_spanned",
];

/// The group at `at` is the input of an `OUTPUT_CODE_MACROS` call: `quote!(..)`.
fn is_output_code(trees: &[TokenTree], at: usize) -> bool {
    at >= 2
        && matches!(&trees[at - 1], TokenTree::Punct(bang) if bang.as_char() == '!')
        && matches!(&trees[at - 2], TokenTree::Ident(name)
            if OUTPUT_CODE_MACROS.iter().any(|m| name == m))
}

/// The edits that fix `super` paths of moved code: it moved one level deeper (into a part
/// file), so every `super::` chain and every `pub(super)` needs one extra `super`. Only
/// tokens are edited; literals, comments and the input of `quote!`-like macros keep their
/// text.
fn super_edits(tokens: TokenStream, edits: &mut Vec<(Range<usize>, &'static str)>) {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    for (at, tree) in trees.iter().enumerate() {
        match tree {
            TokenTree::Group(_) if is_super_restriction(&trees, at) => {
                edits.push((tree.span().byte_range(), "(in super::super)"));
            }
            TokenTree::Group(_) if is_output_code(&trees, at) => {}
            TokenTree::Group(group) => super_edits(group.stream(), edits),
            TokenTree::Ident(_) if is_super(&trees, at) && is_path_sep(&trees, at + 1) => {
                let chain_start =
                    !(at >= 3 && is_path_sep(&trees, at - 2) && is_super(&trees, at - 3));
                if chain_start {
                    let start = tree.span().byte_range().start;
                    edits.push((start..start, "super::"));
                }
            }
            _ => {}
        }
    }
}

/// The source of an item as it reads in a part.
fn moved_source(item: &SplittableItem) -> Result<String> {
    let text = &item.source_text;
    let tokens: TokenStream = text
        .parse()
        .map_err(|e| anyhow::anyhow!("moved item `{}` does not tokenize: {e}", item.name))?;
    let mut edits = Vec::new();
    super_edits(tokens, &mut edits);
    if let Some(fix) = &item.vis_fix {
        edits.push((fix.range.clone(), fix.replacement));
    }
    edits.sort_by_key(|(range, _)| range.start);

    let mut moved = String::new();
    let mut at = 0;
    for (range, replacement) in edits {
        if range.start < at {
            anyhow::bail!("overlapping edits in moved item `{}`", item.name);
        }
        moved.push_str(&text[at..range.start]);
        moved.push_str(replacement);
        at = range.end;
    }
    moved.push_str(&text[at..]);
    Ok(moved)
}

/// Render the body of a part: its free functions, then its methods grouped by impl header
/// in order of first appearance. [`super_glob`] gives the line that goes before it.
fn render_part(cluster: &Cluster) -> Result<String> {
    let mut lines = Vec::new();

    let mut impl_groups: Vec<(&ImplContext, Vec<&SplittableItem>)> = Vec::new();
    for item in &cluster.items {
        let Some(ctx) = &item.impl_context else {
            lines.push(moved_source(item)?);
            lines.push(String::new());
            continue;
        };
        match impl_groups
            .iter_mut()
            .find(|(group, _)| group.header == ctx.header && group.body_top == ctx.body_top)
        {
            Some((_, methods)) => methods.push(item),
            None => impl_groups.push((ctx, vec![item])),
        }
    }

    for (ctx, methods) in impl_groups {
        lines.push(format!("{}{{", ctx.header));
        if let Some(top) = &ctx.body_top {
            lines.push(indent_rows(top, &[]));
            lines.push(String::new());
        }
        for item in methods {
            lines.push(indent_rows(&moved_source(item)?, &item.literal_rows));
            lines.push(String::new());
        }
        lines.push("}".to_string());
        lines.push(String::new());
    }

    Ok(lines.join("\n"))
}
