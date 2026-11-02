use std::fs;
use std::path::Path;

use anyhow::Result;
use walkdir::WalkDir;

pub fn run(path: &Path, threshold: usize) -> Result<()> {
    let mut results: Vec<(String, usize)> = Vec::new();

    for entry in WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path().extension().is_some_and(|ext| ext == "rs")
        })
    {
        let content = fs::read_to_string(entry.path())?;
        let line_count = content.lines().count();
        if line_count >= threshold {
            results.push((entry.path().display().to_string(), line_count));
        }
    }

    results.sort_by(|a, b| b.1.cmp(&a.1));

    if results.is_empty() {
        println!("No .rs files found exceeding {threshold} lines.");
    } else {
        println!("{:<8} {}", "Lines", "File");
        println!("{:<8} {}", "-----", "----");
        for (path, count) in &results {
            println!("{count:<8} {path}");
        }
    }

    Ok(())
}
