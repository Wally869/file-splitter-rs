mod report;
mod scan;
mod split;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "file_splitter_rs", about = "Split large .rs files into submodules")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan a directory for .rs files exceeding a line threshold
    Scan {
        /// Directory to scan
        path: PathBuf,
        /// Line count threshold (default: 300)
        #[arg(short, long, default_value_t = 300)]
        threshold: usize,
    },
    /// Report all functions/methods in a file with their line spans
    Report {
        /// Rust source file to analyze
        file: PathBuf,
        /// Only print function names, one per line
        #[arg(short, long)]
        names_only: bool,
    },
    /// Split a large .rs file into a submodule with clustered subfiles
    Split {
        /// Rust source file to split
        file: PathBuf,
        /// Max lines per subfile (default: 150)
        #[arg(short, long, default_value_t = 150)]
        budget: usize,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Scan { path, threshold } => scan::run(&path, threshold),
        Command::Report { file, names_only } => report::run(&file, names_only),
        Command::Split { file, budget } => split::run(&file, budget),
    }
}
