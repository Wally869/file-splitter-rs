# Issues

## Open (found in round 3, 2026-11-02)

1. **Comments inside an inline module are dropped.** Comments between the items of a split inline module (e.g. `mod tests { ... }`), and after its last item, are not in the output. Top-level items and impl members keep theirs.
2. **Inner attributes of an inherent impl or an inline module are dropped.** `impl Foo { #![allow(x)] ... }` and `mod tests { #![allow(x)] ... }` lose the `#![...]`.
3. **Comments on an impl's header lines are dropped.** A comment after the `{` of `impl Foo { // note`, or between the outer attributes of an inherent impl, is not copied.

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
