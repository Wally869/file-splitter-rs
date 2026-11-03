# file_splitter_rs

Split a long Rust file into a module of smaller files.

The tool does the mechanical split and names the parts; the output compiles.

## Usage

```sh
file_splitter_rs scan <dir> [--threshold 300]   # list .rs files longer than the threshold
file_splitter_rs report <file>                  # list the functions and methods of a file
file_splitter_rs split <file> [--budget 300]    # split the file into parts of at most <budget> lines
```

Typical run:

```sh
$ file_splitter_rs scan src/ --threshold 300
Lines    File
-----    ----
842      src/server.rs
523      src/parser.rs

$ file_splitter_rs split src/server.rs --budget 300
```

`src/server.rs` becomes a folder:

```
src/server/mod.rs      types, traits, constants, imports, pub functions
src/server/handle.rs   e.g. the handle_* methods
src/server/parse.rs    e.g. the parse_* methods
```

`split` prints the folder and each file it wrote, with its line count. `report` on a part shows what it holds:

```sh
$ file_splitter_rs report src/server/handle.rs
Function             Impl       Start    End      Lines
--------             ----       -----    ---      -----
handle_request       Server     32       40       9
handle_response      Server     43       50       8
```

Where the files go:

- `foo.rs` becomes `foo/mod.rs` with the parts in `foo/`.
- A `mod.rs`, `src/lib.rs` or `src/main.rs` stays the root; the parts go next to it.
- A test, bench, example or `src/bin` target `tests/foo.rs` becomes `tests/foo/main.rs`, so cargo still finds the target. A file in `tests/` that another file declares with `mod foo;` is a module, not a target: it becomes `tests/foo/mod.rs`.

`split` never overwrites an existing file.

## Install

```sh
cargo install --path .
```

## What the output looks like

- `mod.rs` keeps the types, traits, constants, `use` lines and the `pub` free functions, so the module's API stays where it was. When these pass the budget, the `pub` functions move too and `mod.rs` re-exports them.
- A part starts with `use super::*;` when it needs a name of its parent. When the tool cannot know (the parent has a glob import, or the part calls a method of a trait the parent may import), the line has `#[allow(unused_imports)]`.
- Methods sit in an `impl Type { }` block for their type.
- A private function that moves to a part becomes `pub(super)`, so the root and the other parts can still call it. A `pub(super)` function becomes `pub(in super::super)`, and each `super::` path gets one more `super`.
- The input of `quote!`, `quote_spanned!`, `parse_quote!` and `parse_quote_spanned!` does not change: it is code for another module.
- A `pub(crate)`, `pub(super)` or `pub(in ..)` function is re-exported from `mod.rs` only when another file of the crate names it. No import is written that nothing uses.
- Paths from the rest of the crate, such as `crate::server::Server`, do not change.

The output compiles without edits.

## How functions are grouped and named

Function names are cut at each `_` and compared with BM25, then clustered: `parse_header`, `parse_body` and `parse_query` end up together. Small clusters merge into the fewest parts that keep each part file at most the budget. Only a single function larger than the budget makes a larger file.

A part takes the word found in most of its item names (`parse`). It never takes a keyword, the parent module's name, a name the file uses as a path, type or module, or a file name already in the folder. When all its words are taken, it gets a number (`parse_2`).
