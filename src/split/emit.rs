use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;

use super::cluster::{self, Cluster};
use super::parse::{ClassifiedFile, InlineModule};

/// Emit the split files: create directory, write mod.rs and part_N.rs files,
/// then delete the original file.
pub fn emit(
    original_file: &Path,
    _source: &str,
    classified: &ClassifiedFile,
    clusters: &[Cluster],
    budget: usize,
) -> Result<()> {
    let file_stem = original_file
        .file_stem()
        .context("File has no stem")?
        .to_str()
        .context("File stem is not valid UTF-8")?;

    let parent = original_file
        .parent()
        .context("File has no parent directory")?;

    let module_dir = parent.join(file_stem);

    // Create the module directory
    fs::create_dir_all(&module_dir)
        .with_context(|| format!("Failed to create directory {}", module_dir.display()))?;

    // Write each cluster to part_N.rs
    for (i, cluster) in clusters.iter().enumerate() {
        let part_path = module_dir.join(format!("part_{i}.rs"));
        let content = render_subfile(cluster, &classified.use_statements);
        fs::write(&part_path, &content)
            .with_context(|| format!("Failed to write {}", part_path.display()))?;
    }

    // Emit inline modules (e.g. #[cfg(test)] mod tests { ... })
    for inline_mod in &classified.inline_modules {
        emit_inline_module(&module_dir, inline_mod, budget)?;
    }

    // Write mod.rs
    let mod_content = render_mod_file(classified, clusters);
    let mod_path = module_dir.join("mod.rs");
    fs::write(&mod_path, &mod_content)
        .with_context(|| format!("Failed to write {}", mod_path.display()))?;

    // Delete the original file
    fs::remove_file(original_file)
        .with_context(|| format!("Failed to delete {}", original_file.display()))?;

    Ok(())
}

/// Emit an inline module as a subdirectory with its own mod.rs and part_N.rs files.
fn emit_inline_module(
    parent_dir: &Path,
    inline_mod: &InlineModule,
    budget: usize,
) -> Result<()> {
    let mod_dir = parent_dir.join(&inline_mod.name);
    fs::create_dir_all(&mod_dir)
        .with_context(|| format!("Failed to create directory {}", mod_dir.display()))?;

    // Cluster the inline module's items
    let clusters = cluster::cluster_items(&inline_mod.splittable_items, budget);

    // Write part_N.rs files — these are TWO levels deeper than the original file
    // (file.rs → file/mod_name/part_N.rs), so super:: needs two extra levels
    for (i, cluster) in clusters.iter().enumerate() {
        let part_path = mod_dir.join(format!("part_{i}.rs"));
        let content = render_inline_subfile(cluster, &inline_mod.use_statements);
        fs::write(&part_path, &content)
            .with_context(|| format!("Failed to write {}", part_path.display()))?;
    }

    // Write the inline module's mod.rs
    let mod_content = render_inline_mod_file(inline_mod, &clusters);
    let mod_path = mod_dir.join("mod.rs");
    fs::write(&mod_path, &mod_content)
        .with_context(|| format!("Failed to write {}", mod_path.display()))?;

    println!(
        "  Inline module '{}': {} clusters",
        inline_mod.name,
        clusters.len()
    );
    for (i, c) in clusters.iter().enumerate() {
        let names: Vec<&str> = c.items.iter().map(|item| item.name.as_str()).collect();
        println!("    part_{i}: {} items — {:?}", c.items.len(), names);
    }

    Ok(())
}

/// Fix `super::` paths in source text: since the code moved one level deeper
/// (from `file.rs` to `file/part_N.rs`), every `super::` chain needs one extra
/// `super::` prepended.
fn fix_super_paths(text: &str) -> String {
    let re = Regex::new(r"((?:super::)+)").unwrap();
    re.replace_all(text, "super::$1").to_string()
}

/// Render a subfile containing a cluster of functions/methods.
fn render_subfile(cluster: &Cluster, use_statements: &[String]) -> String {
    let mut lines = Vec::new();

    // Add use statements (with super:: paths fixed)
    for stmt in use_statements {
        lines.push(fix_super_paths(stmt));
    }
    if !use_statements.is_empty() {
        lines.push(String::new());
    }

    // Group by impl context — items sharing the same impl header go together
    let mut impl_groups: HashMap<Option<String>, Vec<&str>> = HashMap::new();

    for item in &cluster.items {
        let key = item.impl_context.as_ref().map(|ctx| ctx.header.clone());
        impl_groups
            .entry(key)
            .or_default()
            .push(&item.source_text);
    }

    // Emit free functions first (with super:: paths fixed)
    if let Some(free_fns) = impl_groups.remove(&None) {
        for source in free_fns {
            lines.push(fix_super_paths(source));
            lines.push(String::new());
        }
    }

    // Emit impl-grouped methods (with super:: paths fixed)
    for (header_opt, methods) in impl_groups {
        let header = header_opt.unwrap(); // we already removed None
        lines.push(format!("{header} {{"));
        for source in methods {
            let fixed = fix_super_paths(source);
            for line in fixed.lines() {
                lines.push(format!("    {line}"));
            }
            lines.push(String::new());
        }
        lines.push("}".to_string());
        lines.push(String::new());
    }

    lines.join("\n")
}

/// Render a subfile for an inline module's cluster.
/// These are TWO levels deeper than the original, so super:: gets two extra levels.
fn render_inline_subfile(cluster: &Cluster, use_statements: &[String]) -> String {
    let mut lines = Vec::new();

    // Add use statements (fix super:: paths: two levels deeper)
    for stmt in use_statements {
        let fixed = fix_super_paths(&fix_super_paths(stmt));
        lines.push(fixed);
    }
    if !use_statements.is_empty() {
        lines.push(String::new());
    }

    // Group by impl context
    let mut impl_groups: HashMap<Option<String>, Vec<&str>> = HashMap::new();
    for item in &cluster.items {
        let key = item.impl_context.as_ref().map(|ctx| ctx.header.clone());
        impl_groups.entry(key).or_default().push(&item.source_text);
    }

    // Free functions (fix super:: paths: two levels deeper)
    if let Some(free_fns) = impl_groups.remove(&None) {
        for source in free_fns {
            let fixed = fix_super_paths(&fix_super_paths(source));
            lines.push(fixed);
            lines.push(String::new());
        }
    }

    // Impl-grouped methods
    for (header_opt, methods) in impl_groups {
        let header = header_opt.unwrap();
        lines.push(format!("{header} {{"));
        for source in methods {
            let fixed = fix_super_paths(&fix_super_paths(source));
            for line in fixed.lines() {
                lines.push(format!("    {line}"));
            }
            lines.push(String::new());
        }
        lines.push("}".to_string());
        lines.push(String::new());
    }

    lines.join("\n")
}

/// Render mod.rs for an inline module subdirectory.
fn render_inline_mod_file(inline_mod: &InlineModule, clusters: &[Cluster]) -> String {
    let mut lines = Vec::new();

    // Use statements (fix super:: one level for being inside the subdir)
    for stmt in &inline_mod.use_statements {
        lines.push(fix_super_paths(stmt));
    }
    if !inline_mod.use_statements.is_empty() {
        lines.push(String::new());
    }

    // Non-function items (consts, types, etc.)
    for item in &inline_mod.mod_items {
        lines.push(fix_super_paths(&item.source_text));
        lines.push(String::new());
    }

    // Module declarations and re-exports
    for i in 0..clusters.len() {
        lines.push(format!("mod part_{i};"));
    }
    lines.push(String::new());

    for i in 0..clusters.len() {
        lines.push(format!("pub use part_{i}::*;"));
    }
    lines.push(String::new());

    lines.join("\n")
}

/// Render the mod.rs file with definitions, mod declarations, and re-exports.
fn render_mod_file(classified: &ClassifiedFile, clusters: &[Cluster]) -> String {
    let mut lines = Vec::new();

    // Write all mod.rs items (structs, enums, traits, consts, use, etc.)
    for item in &classified.mod_items {
        lines.push(item.source_text.clone());
        lines.push(String::new());
    }

    if !classified.mod_items.is_empty() {
        lines.push(String::new());
    }

    // Module declarations and re-exports for split clusters
    for i in 0..clusters.len() {
        lines.push(format!("mod part_{i};"));
    }

    lines.push(String::new());

    for i in 0..clusters.len() {
        lines.push(format!("pub use part_{i}::*;"));
    }

    lines.push(String::new());

    // Inline module declarations (e.g. `#[cfg(test)] mod tests;`)
    for inline_mod in &classified.inline_modules {
        if !inline_mod.attrs_source.is_empty() {
            lines.push(inline_mod.attrs_source.clone());
        }
        lines.push(format!("mod {};", inline_mod.name));
        lines.push(String::new());
    }

    lines.join("\n")
}
