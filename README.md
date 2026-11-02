# file_splitter_rs

`file_splitter_rs` splits a large Rust source file into a module: a root `mod.rs` and a few part files, each holding a group of related functions.

Coding agents work badly on very long files. They lose track of the content and fall back on fragile `sed` edits. This tool does the mechanical part of a split, so the agent only has to name the new files.

## Install

```sh
cargo install --path .
```

## Commands

```
file_splitter_rs scan <path> [--threshold <N>]     # default threshold: 300
file_splitter_rs report <file>
file_splitter_rs split <file> [--budget <N>]       # default budget: 150
```

### scan

Lists the `.rs` files under a directory that are longer than the threshold, longest first.

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

### report

Lists each function and method in a file, with its `impl` type and its line span.

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

### split

Turns one file into a module. The budget is the target size of a part, in lines.

```sh
file_splitter_rs split src/server.rs --budget 150
```

`src/server.rs` is replaced by:

```
src/server/mod.rs       types, traits, constants, imports, re-exports
src/server/part_0.rs    for example the handle_* methods
src/server/part_1.rs    for example the parse_* methods
```

## How a split works

1. **Parse.** `syn` reads the file and lists its top-level items.
2. **Sort.** Type definitions, traits, constants and `use` lines stay in `mod.rs`. Free functions and `impl` methods can move.
3. **Group.** Function names are cut at each `_` and compared with BM25. Agglomerative clustering then groups the closest names, so `parse_header`, `parse_body` and `parse_query` end up in the same part.
4. **Write.** Each group becomes a `part_N.rs` file. Methods stay inside an `impl Type { }` block of their type. `mod.rs` re-exports each part with `pub use part_N::*`.

Because of these re-exports, paths such as `use crate::server::Server` keep working in the rest of the crate.

## Use by an agent

1. `file_splitter_rs scan src/ --threshold 300` to find the long files.
2. `file_splitter_rs split src/big_file.rs --budget 150` to split one.
3. `file_splitter_rs report src/big_file/part_0.rs` to see what a part holds.
4. Rename each part after its content, for example `part_0.rs` to `handlers.rs`, and change its `mod` line in `mod.rs` to match.
