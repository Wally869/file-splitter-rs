# file_splitter_rs

A CLI tool that splits large Rust source files into submodules with semantically clustered subfiles. Designed for AI-assisted codebases where agents struggle with oversized files.

## Problem

When `.rs` files grow too large, AI coding agents lose context and resort to fragile `sed`-based edits. This tool automates splitting those files into well-organized submodules while keeping imports stable.

## Installation

```sh
cargo install --path .
```

## Usage

### Scan for oversized files

Find all `.rs` files in a directory that exceed a line threshold:

```sh
file_splitter_rs scan src/ --threshold 300
```

```
Lines    File
-----    ----
842      src/server.rs
523      src/parser.rs
311      src/utils.rs
```

### Report functions in a file

List every function and method with its line span and parent impl:

```sh
file_splitter_rs report src/server.rs
```

```
Function                                 Impl                           Start    End      Lines
--------                                 ----                           -----    ---      -----
new                                      Server                         24       29       6
handle_request                           Server                         32       40       9
handle_response                          Server                         43       50       8
validate_port                            -                              140      142      3
```

### Split a file into a submodule

Split a large file into clustered subfiles:

```sh
file_splitter_rs split src/server.rs --budget 150
```

This transforms:

```
src/server.rs           → deleted
```

Into:

```
src/server/mod.rs       → struct/enum/trait definitions + re-exports
src/server/part_0.rs    → handle_* methods
src/server/part_1.rs    → parse_* methods
src/server/part_2.rs    → validate_* functions
...
```

## How splitting works

1. **Parse** — The file is parsed with `syn` to extract all top-level items
2. **Classify** — Structs, enums, traits, consts, and use statements stay in `mod.rs`. Functions and impl methods get split out
3. **Cluster** — Function names are tokenized (split on `_`) and grouped using BM25 similarity scoring with agglomerative clustering. Functions with similar name prefixes (e.g. `parse_header`, `parse_body`, `parse_query`) naturally cluster together
4. **Emit** — Each cluster becomes a `part_N.rs` file. Methods are wrapped in their original `impl Type { }` blocks. `mod.rs` re-exports everything via `pub use part_N::*`

### What stays stable

Downstream imports don't break. If other files use `use crate::server::SomeStruct`, that path still works because `mod.rs` re-exports all public items.

## Intended workflow

```
1. agent runs:  file_splitter_rs scan src/ --threshold 300
2. agent runs:  file_splitter_rs split src/big_file.rs --budget 150
3. agent runs:  file_splitter_rs report src/big_file/part_0.rs
4. agent renames part_0.rs → handlers.rs based on contents
5. agent updates mod.rs declarations to match new filenames
```

## CLI Reference

```
file_splitter_rs scan <path> [--threshold <N>]     # default threshold: 300
file_splitter_rs report <file>
file_splitter_rs split <file> [--budget <N>]        # default budget: 150
```
