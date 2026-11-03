mod cluster;
mod emit;
mod names;
mod parse;

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};

pub fn run(file: &Path, budget: usize) -> Result<()> {
    let source = std::fs::read_to_string(file)
        .with_context(|| format!("Failed to read {}", file.display()))?;

    let parsed = syn::parse_file(&source)
        .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", file.display(), e))?;

    // 1. Parse and classify items
    let classified = parse::classify_items(&parsed, &source)?;

    // The root lines of the items that never move, each with its blank line.
    let lines = |text: &str| text.lines().count() + 1;
    let fixed_lines = lines(&classified.preamble)
        + classified
            .mod_items
            .iter()
            .map(|item| lines(&item.source_text))
            .sum::<usize>()
        + classified
            .inline_modules
            .iter()
            .map(|inline| lines(&inline.declaration))
            .sum::<usize>()
        + classified.epilogue.as_deref().map_or(0, lines);
    let (kept, moved) =
        cluster::split_root_items(&classified.splittable_items, budget, fixed_lines);

    if moved.is_empty() && classified.inline_modules.is_empty() {
        println!("No functions/methods to split in {}", file.display());
        return Ok(());
    }

    println!(
        "Found {} items to split, {} items and {} pub functions stay in the root",
        moved.len(),
        classified.mod_items.len(),
        kept.len()
    );

    // 2. Cluster splittable items by name similarity
    let clusters = cluster::cluster_items(&moved, budget);

    println!("Created {} clusters", clusters.len());

    // 3. Emit files
    let mut source_clashes = HashSet::new();
    let tokens: proc_macro2::TokenStream = source
        .parse()
        .map_err(|e| anyhow::anyhow!("Failed to tokenize {}: {e}", file.display()))?;
    names::module_clashes(tokens, &mut source_clashes);
    let emitted = emit::emit(file, &classified, &kept, &clusters, budget, &source_clashes)?;

    println!(
        "\nSplit {} into {} subfiles",
        file.display(),
        clusters.len()
    );
    println!("Output directory: {}", emitted.dir.display());
    for written in &emitted.files {
        println!("  {} ({} lines)", written.path.display(), written.lines);
    }

    Ok(())
}
