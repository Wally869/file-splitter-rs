# file_splitter_rs

CLI tool to split large `.rs` files into submodules with semantically clustered subfiles.

## Codemap

```
src/
  main.rs              — CLI entry (clap): scan, report, split subcommands
  scan.rs              — `scan <path> --threshold N`: find .rs files exceeding N lines
  report.rs            — `report <file>`: list all functions/methods with line spans
  split/
    mod.rs             — `split <file> --budget N`: orchestrates parse → cluster → emit
    parse.rs           — syn-based AST parsing, item classification (mod vs splittable)
    cluster.rs         — BM25 name similarity + agglomerative clustering
    emit.rs            — renders clusters to part_N.rs files + mod.rs with re-exports
```

## Build & Run

```sh
cargolite check
cargolite build
cargo run -- scan <path> --threshold 300
cargo run -- report <file>
cargo run -- split <file> --budget 150
```
