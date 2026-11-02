# Issues

## Open (found in round 2, 2026-11-02)

1. **Comments outside items are dropped.** Plain `//` comments between top-level items (after the first item), and between the methods of an inherent impl, are not in the output. Items are cut by their token spans and comments are not tokens.
2. **Attributes on an inherent impl are dropped.** `#[cfg(...)] impl Foo { ... }`: the moved methods and the partial impl in the root get a bare `impl Foo` header.
3. **Inline module visibility is dropped.** `pub mod inner { ... }` becomes `mod inner;` in the root.
4. **`super::` inside string literals and comments is rewritten.** The `super::` fix of moved code is a regex over the text, so it also changes literal content.
5. **The cooking-frenzy re-check was not run in round 2.** It needs a scratch `git worktree` in another repository, which the round 2 agent was not allowed to run.

## Fixed in round 2 (commits 925b7a1 and e65448a, 2026-11-02)

- A trait impl moves as one block, with its associated types and consts; inherent impls are still split by method (925b7a1).
- No item is dropped: `extern crate`, `union`, `extern {}` blocks, `macro_rules!` and associated consts and types of inherent impls stay in the root, at any position (925b7a1).
- One dedent rule for free functions, methods and items of inline modules; rows that start inside a literal keep their text (925b7a1).
- Tests compare the items of input and output on the compile fixture and on `tests/fixtures/mixed.rs` (925b7a1).
- `src/split/parse.rs` (569 lines) is split by the tool itself into `parse/{mod,fn_source,inline_module,impls}.rs`: 182, 111, 144 and 92 lines (e65448a).

## Fixed in round 1 (merge of 76ed3bb, 2026-11-02)

The `cooking-frenzy/crates/dsl/src/reach/mod.rs` split (682 lines, budget 150) now compiles with no hand fixes: 4 parts, `cargo check -p dsl` clean.

- A `mod.rs` input stays the root; parts go beside it.
- Parts start with `use super::*;`, not a copy of the root header.
- Private items used across parts get `pub(super)`.
- `split` prints the output folder and each file with its line count.
- Part count is ceil(moved lines / budget); the file's `pub` free functions stay in the root.
