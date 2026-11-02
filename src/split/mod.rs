mod cluster;
mod emit;
mod parse;

use std::path::Path;

use anyhow::{Context, Result};

pub fn run(file: &Path, budget: usize) -> Result<()> {
    let source = std::fs::read_to_string(file)
        .with_context(|| format!("Failed to read {}", file.display()))?;

    let parsed = syn::parse_file(&source)
        .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", file.display(), e))?;

    // 1. Parse and classify items
    let classified = parse::classify_items(&parsed, &source)?;

    let (kept, moved) = cluster::split_root_items(&classified.splittable_items, budget);

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
    for (i, c) in clusters.iter().enumerate() {
        let names: Vec<&str> = c.items.iter().map(|item| item.name.as_str()).collect();
        println!(
            "  part_{i}: {} items, {} lines — {:?}",
            c.items.len(),
            c.total_lines,
            names
        );
    }

    // 3. Emit files
    let emitted = emit::emit(file, &classified, &kept, &clusters, budget)?;

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
