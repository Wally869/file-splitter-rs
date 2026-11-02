# Issues

## Open (found in round 5, 2026-11-02)

1. **Tests share one scratch folder.** The integration tests copy fixtures to `~/.cargo/build/tmp/<case>`, a path shared by every worktree; two test runs at the same time can overwrite each other's files.
2. **`tests/split_cli.rs` is about 545 lines**, above the project's 500-line limit.
3. Low: the doc comment of `ImplContext.header` (`src/split/parse/mod.rs`) is out of date; the header now holds the outer attributes and the whitespace before `{`. Not a defect; not scheduled.

## Fixed in round 5 (commit 3512f7a, 2026-11-02)

- The header of an inherent impl is copied from the source text as written, from its first outer attribute to its `{`: generics, `where` clause (also over several lines), `unsafe`, `default`, the comments in it and the whitespace before the `{` are the same on every block emitted for it.

## Fixed in round 4 (commit 1fee84f, 2026-11-02)

- Comments between the items of a split inline module move with the item that follows; comments after its last item stay at the end of the module's `mod.rs`.
- Inner attributes are kept: the `#![...]` of an inherent impl is on every block emitted for it; the `#![...]` of a split inline module opens its `mod.rs`.
- Comments on an inherent impl's header lines are on every block emitted for it: a comment after the `{` (with the inner attributes, at the top of the body) and comments between the outer attributes.

## Fixed in round 3 (commit be46da4, 2026-11-02)

- Comments between top-level items and between the members of an inherent impl move with the item that follows; comments after the last item stay at the end of the root or of the partial impl in the root.
- The outer attributes of an inherent impl are on every block emitted for it: the partial impl in the root and each part's impl.
- An inline module's root declaration keeps its comments, attributes and visibility as written (`#[allow(dead_code)]\npub mod shapes;`).
- The `super` fix of moved code edits path tokens only (`proc-macro2` tokens with byte spans); string, raw string and char literals and comments keep their text.

## Fixed in round 2 (commits 925b7a1 and e65448a, 2026-11-02)

- A trait impl moves as one block, with its associated types and consts; inherent impls are still split by method (925b7a1).
- No item is dropped: `extern crate`, `union`, `extern {}` blocks, `macro_rules!` and associated consts and types of inherent impls stay in the root, at any position (925b7a1).
- One dedent rule for free functions, methods and items of inline modules; rows that start inside a literal keep their text (925b7a1).
- Tests compare the items of input and output on the compile fixture and on `tests/fixtures/mixed.rs` (925b7a1).
- The cooking-frenzy `reach/mod.rs` split still compiles after round 2: 4 parts, `cargo check -p dsl` clean (run by the CTO after the merge).
- `src/split/parse.rs` (569 lines) is split by the tool itself into `parse/{mod,fn_source,inline_module,impls}.rs`: 182, 111, 144 and 92 lines (e65448a).

## Fixed in round 1 (merge of 76ed3bb, 2026-11-02)

The `cooking-frenzy/crates/dsl/src/reach/mod.rs` split (682 lines, budget 150) now compiles with no hand fixes: 4 parts, `cargo check -p dsl` clean.

- A `mod.rs` input stays the root; parts go beside it.
- Parts start with `use super::*;`, not a copy of the root header.
- Private items used across parts get `pub(super)`.
- `split` prints the output folder and each file with its line count.
- Part count is ceil(moved lines / budget); the file's `pub` free functions stay in the root.
