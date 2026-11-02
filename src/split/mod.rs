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
    let classified = parse::classify_items(&parsed, &source);

    if classified.splittable_items.is_empty() {
        println!("No functions/methods to split in {}", file.display());
        return Ok(());
    }

    println!(
        "Found {} items to split, {} items stay in mod.rs",
        classified.splittable_items.len(),
        classified.mod_items.len()
    );

    // 2. Cluster splittable items by name similarity
    let clusters = cluster::cluster_items(&classified.splittable_items, budget);

    println!("Created {} clusters", clusters.len());
    for (i, c) in clusters.iter().enumerate() {
        let names: Vec<&str> = c.items.iter().map(|item| item.name.as_str()).collect();
        println!("  part_{i}: {} items — {:?}", c.items.len(), names);
    }

    // 3. Emit files
    emit::emit(file, &source, &classified, &clusters, budget)?;

    println!(
        "\nSplit {} into {} subfiles",
        file.display(),
        clusters.len()
    );

    Ok(())
}
