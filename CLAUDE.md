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
    parse/             — syn-based parsing, item classification (root vs movable), source spans
      mod.rs           — entry, item and comment spans
      fn_source.rs     — source text of moved functions, dedent
      impls.rs         — impl headers and members (inherent split, trait impls whole)
      inline_module.rs — inline `mod x { }` blocks
    cluster.rs         — BM25 name similarity + agglomerative clustering, budget is a max per file
    names.rs           — part names from item words, identifiers that clash with a module
    emit.rs            — file layout (mod.rs, crate roots, test targets), root imports and
                         re-exports, `use super::*;` when needed, token-aware super:: rewrite
                         (skips quote!-like input)
tests/
  split_cli/           — integration tests (main.rs root, helpers.rs, items, source_text, headers, layout)
  fixtures/            — split_crate/ (must compile after a split, with -D unused-imports;
                         tests/flow.rs is a test target, tests/shared.rs a module of suite.rs),
                         mixed.rs
```

Open defects and fix history: `ISSUES.md`.

## Build & Run

```sh
cargolite check
cargolite build
cargo run -- scan <path> --threshold 300
cargo run -- report <file>
cargo run -- split <file> --budget 300
```
