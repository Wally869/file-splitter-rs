# Issues

## Open (found in round 1, 2026-11-02)

1. **Trait impl methods are split across parts.** This gives several impls of the same trait for one type, which does not compile. When the impl has associated items, a partial impl also stays in the root and clashes. Expected: a trait impl moves as one block.
2. **Items with no span are dropped.** `extern crate`, `union` and `extern {}` blocks disappear from the output, unless one is the first item of the file. Expected: they stay in the root.
3. **Inline module methods are not dedented.** Their free functions are dedented, and that can change multi-line string literals. Expected: one dedent rule for both that never touches literal content.
4. **`src/split/parse.rs` is 569 lines.** Low priority: it works, but it is above the project's 500-line limit for files.

## Fixed in round 1 (merge of 76ed3bb, 2026-11-02)

The `cooking-frenzy/crates/dsl/src/reach/mod.rs` split (682 lines, budget 150) now compiles with no hand fixes: 4 parts, `cargo check -p dsl` clean.

- A `mod.rs` input stays the root; parts go beside it.
- Parts start with `use super::*;`, not a copy of the root header.
- Private items used across parts get `pub(super)`.
- `split` prints the output folder and each file with its line count.
- Part count is ceil(moved lines / budget); the file's `pub` free functions stay in the root.
