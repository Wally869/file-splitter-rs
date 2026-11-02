use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result};
use regex::Regex;

use super::cluster::{self, Cluster};
use super::parse::{ClassifiedFile, InlineModule, SplittableItem, indent_rows};

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

/// Emit the split files. A `<dir>/mod.rs` source stays the root and its parts go beside it
/// in `<dir>/`; a `<dir>/foo.rs` source becomes `<dir>/foo/mod.rs` with the parts in
/// `<dir>/foo/`, and the original file is deleted.
pub fn emit(
    original_file: &Path,
    classified: &ClassifiedFile,
    kept: &[SplittableItem],
    clusters: &[Cluster],
    budget: usize,
) -> Result<Emitted> {
    let parent = original_file
        .parent()
        .context("File has no parent directory")?;
    let is_mod_rs = original_file
        .file_name()
        .is_some_and(|name| name == "mod.rs");

    let (module_dir, root_path) = if is_mod_rs {
        (parent.to_path_buf(), original_file.to_path_buf())
    } else {
        let file_stem = original_file
            .file_stem()
            .context("File has no stem")?
            .to_str()
            .context("File stem is not valid UTF-8")?;
        let module_dir = parent.join(file_stem);
        let root_path = module_dir.join("mod.rs");
        (module_dir, root_path)
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
        if !inline_mod.attrs_source.is_empty() {
            tail.push(inline_mod.attrs_source.clone());
        }
        tail.push(format!("mod {};", inline_mod.name));
    }

    let mut planned = Vec::new();
    plan_module(
        &module_dir,
        root_path.clone(),
        &head,
        clusters,
        &tail,
        &mut planned,
    );
    for inline_mod in &classified.inline_modules {
        plan_inline_module(&module_dir, inline_mod, budget, &mut planned);
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

    if !is_mod_rs {
        fs::remove_file(original_file)
            .with_context(|| format!("Failed to delete {}", original_file.display()))?;
    }

    Ok(Emitted {
        dir: module_dir,
        files,
    })
}

/// Plan an inline module (e.g. `#[cfg(test)] mod tests { ... }`) as a subdirectory with its
/// own mod.rs and part_N.rs files. It stays a child of the same module, so its own `use`
/// lines and items keep their paths.
fn plan_inline_module(
    parent_dir: &Path,
    inline_mod: &InlineModule,
    budget: usize,
    planned: &mut Vec<(PathBuf, String)>,
) {
    let mod_dir = parent_dir.join(&inline_mod.name);
    let clusters = cluster::cluster_items(&inline_mod.splittable_items, budget);

    let mut head = inline_mod.use_statements.clone();
    head.extend(
        inline_mod
            .mod_items
            .iter()
            .map(|item| item.source_text.clone()),
    );
    let root_path = mod_dir.join("mod.rs");
    plan_module(&mod_dir, root_path, &head, &clusters, &[], planned);

    println!(
        "  Inline module '{}': {} clusters",
        inline_mod.name,
        clusters.len()
    );
    for (i, c) in clusters.iter().enumerate() {
        let names: Vec<&str> = c.items.iter().map(|item| item.name.as_str()).collect();
        println!("    part_{i}: {} items — {:?}", c.items.len(), names);
    }
}

/// Plan a module root and its parts. The root is `head`, the part declarations with the
/// imports of moved free functions, then `tail`.
fn plan_module(
    dir: &Path,
    root_path: PathBuf,
    head: &[String],
    clusters: &[Cluster],
    tail: &[String],
    planned: &mut Vec<(PathBuf, String)>,
) {
    let parts: Vec<String> = clusters.iter().map(render_part).collect();
    let head_text = head.join("\n\n");

    let mut lines = Vec::new();
    if !head.is_empty() {
        lines.push(head_text.clone());
        lines.push(String::new());
    }
    for i in 0..clusters.len() {
        lines.push(format!("mod part_{i};"));
    }
    if !clusters.is_empty() {
        lines.push(String::new());
    }
    let imports = part_imports(&head_text, clusters, &parts);
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
    for (i, content) in parts.into_iter().enumerate() {
        planned.push((dir.join(format!("part_{i}.rs")), content));
    }
}

/// The root's `use` lines for moved free functions: an item with a visibility keeps it as a
/// re-export; a private one is imported when the root or another part names it, so the
/// other parts reach it through `use super::*;`.
fn part_imports(head_text: &str, clusters: &[Cluster], parts: &[String]) -> Vec<String> {
    let mut imports = Vec::new();
    for (i, cluster) in clusters.iter().enumerate() {
        for item in cluster
            .items
            .iter()
            .filter(|item| item.impl_context.is_none() && !item.trait_impl)
        {
            let name = &item.name;
            if !item.root_vis.is_empty() {
                imports.push(format!("{} use part_{i}::{name};", item.root_vis));
                continue;
            }
            let word = Regex::new(&format!(r"\b{}\b", regex::escape(name)))
                .expect("an escaped identifier is a valid regex");
            let named_outside = word.is_match(head_text)
                || parts
                    .iter()
                    .enumerate()
                    .any(|(j, part)| j != i && word.is_match(part));
            if named_outside {
                imports.push(format!("use part_{i}::{name};"));
            }
        }
    }
    imports
}

static SUPER_CHAIN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"((?:super::)+)").expect("valid regex"));
static PUB_SUPER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"pub\(\s*(?:in\s+)?super\s*\)").expect("valid regex"));

/// Fix `super` paths in moved source text: the code moved one level deeper (into
/// `part_N.rs`), so every `super::` chain and every `pub(super)` needs one extra `super`.
fn fix_super_paths(text: &str) -> String {
    let chains = SUPER_CHAIN.replace_all(text, "super::$1");
    PUB_SUPER
        .replace_all(&chains, "pub(in super::super)")
        .to_string()
}

/// The source of an item as it reads in a part.
fn moved_source(item: &SplittableItem) -> String {
    let text = &item.source_text;
    match &item.vis_fix {
        None => fix_super_paths(text),
        Some(fix) => format!(
            "{}{}{}",
            fix_super_paths(&text[..fix.range.start]),
            fix.replacement,
            fix_super_paths(&text[fix.range.end..])
        ),
    }
}

/// Render a part: `use super::*;`, its free functions, then its methods grouped by impl
/// header in order of first appearance.
fn render_part(cluster: &Cluster) -> String {
    let mut lines = vec!["use super::*;".to_string(), String::new()];

    let mut impl_groups: Vec<(&str, Vec<&SplittableItem>)> = Vec::new();
    for item in &cluster.items {
        let Some(ctx) = &item.impl_context else {
            lines.push(moved_source(item));
            lines.push(String::new());
            continue;
        };
        match impl_groups
            .iter_mut()
            .find(|(header, _)| *header == ctx.header)
        {
            Some((_, methods)) => methods.push(item),
            None => impl_groups.push((&ctx.header, vec![item])),
        }
    }

    for (header, methods) in impl_groups {
        lines.push(format!("{header} {{"));
        for item in methods {
            lines.push(indent_rows(&moved_source(item), &item.literal_rows));
            lines.push(String::new());
        }
        lines.push("}".to_string());
        lines.push(String::new());
    }

    lines.join("\n")
}
