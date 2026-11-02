# Issues

## 2026-11-02: split of `cooking-frenzy/crates/dsl/src/reach/mod.rs` (682 lines, budget 150)

Found by the `file-splitter` agent. The split needed hand fixes before it compiled (cooking-frenzy commit e11e817).

1. **A `mod.rs` source breaks the module.** The tool moved the root to `reach/mod/mod.rs`, wrote the parts in `reach/mod/` and deleted `reach/mod.rs`. Expected: the parts go beside `mod.rs` in `reach/`, and `mod.rs` stays the root.
2. **Each part copies the whole `use` header.** Paths that are relative to the root (`use expand::At;`, `use sets::...;`) do not resolve in a submodule; they need `super::`. The copied header also leaves many unused imports. The agent replaced each part's header with `use super::*;`.
3. **Methods called from another part stay private.** An inherent method that moves to one part and is called from another part does not get `pub(super)`, so the cross-part calls fail to compile.
4. **`split` does not print the output directory.** With issue 1, the parts went to a directory the caller did not expect.
5. **The cluster layout ignores the suggested cut.** For example, `reach` and `reach_sources` moved out of `mod.rs` into `entry.rs`, and 12 small parts were made where 2 were suggested.
