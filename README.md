# file_splitter_rs

Split a long Rust file into a module of smaller files.

Coding agents handle long files badly: they lose track of the content and fall back on fragile `sed` edits. Asking a strong model to do the split itself wastes tokens on mechanical work. This tool does that work in one command, and the output compiles. What is left is to give the parts good names, and a cheap agent such as Haiku can do that.

## Usage

```sh
file_splitter_rs scan <dir> [--threshold 300]   # list .rs files longer than the threshold
file_splitter_rs report <file>                  # list the functions and methods of a file
file_splitter_rs split <file> [--budget 150]    # split the file into parts of about <budget> lines
```

Typical run:

```sh
$ file_splitter_rs scan src/ --threshold 300
Lines    File
-----    ----
842      src/server.rs
523      src/parser.rs

$ file_splitter_rs split src/server.rs --budget 150
```

`src/server.rs` becomes a folder:

```
src/server/mod.rs      types, traits, constants, imports, pub functions
src/server/part_0.rs   e.g. the handle_* methods
src/server/part_1.rs   e.g. the parse_* methods
```

`split` prints the folder and each file it wrote. Rename the parts after what they hold (`part_0.rs` to `handlers.rs`) and change the matching `mod` line in `mod.rs`. `report` on a part shows what it holds:

```sh
$ file_splitter_rs report src/server/part_0.rs
Function             Impl       Start    End      Lines
--------             ----       -----    ---      -----
handle_request       Server     32       40       9
handle_response      Server     43       50       8
```

If the input is already a `mod.rs`, it stays the root and the parts go next to it. `split` never overwrites an existing file.

## Install

```sh
cargo install --path .
```

## What the output looks like

- `mod.rs` keeps the types, traits, constants, `use` lines and the `pub` free functions, so the module's API stays where it was.
- Each part starts with `use super::*;`. Methods sit in an `impl Type { }` block for their type.
- A private function that moves to a part becomes `pub(super)`, so the root and the other parts can still call it.
- Paths from the rest of the crate, such as `crate::server::Server`, do not change.

The output compiles without edits.

## How functions are grouped

Function names are cut at each `_` and compared with BM25, then clustered: `parse_header`, `parse_body` and `parse_query` end up together. Small clusters merge until the number of parts is the moved lines divided by the budget, rounded up.
