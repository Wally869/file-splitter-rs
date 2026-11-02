use std::ops::Range;

use anyhow::{Context, Result};

use proc_macro2::{LineColumn, TokenStream, TokenTree};

use quote::ToTokens;

use syn::{ImplItem, Item, Visibility};

/// A single splittable item (function, method or whole trait impl) with its source text.
#[derive(Debug, Clone)]
pub struct SplittableItem {
    /// Function/method name
    pub name: String,
    /// The raw source text of this item (including attrs/docs)
    pub source_text: String,
    /// Line count of the source text
    pub line_count: usize,
    /// If this is a method, the impl block context needed to wrap it
    pub impl_context: Option<ImplContext>,
    /// A whole `impl Trait for Type` block: a trait impl cut in pieces does not compile
    pub trait_impl: bool,
    /// Declared `pub` (not restricted)
    pub public: bool,
    /// The visibility as written in the source module, empty when private
    pub root_vis: String,
    /// The edit that keeps a private item reachable once it moves into a part
    pub vis_fix: Option<VisFix>,
    /// Rows of `source_text` that start inside a literal, where an indent change would
    /// change the literal
    pub literal_rows: Vec<usize>,
}

/// Replaces `range` of `source_text` with `replacement` when the item is emitted in a part.
#[derive(Debug, Clone)]
pub struct VisFix {
    pub range: Range<usize>,
    pub replacement: &'static str,
}

/// Context needed to reconstruct an impl block around extracted methods.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ImplContext {
    /// The impl header, e.g. `impl<T: Clone> MyStruct<T>`
    pub header: String,
    /// The self type as a string for display
    pub self_type: String,
}

/// Items that stay in mod.rs, stored as raw source text.
#[derive(Debug, Clone)]
pub struct ModItem {
    pub source_text: String,
}

/// An inline module (e.g. `#[cfg(test)] mod tests { ... }`) whose contents should be split.
#[derive(Debug, Clone)]
pub struct InlineModule {
    /// Module name, e.g. "tests"
    pub name: String,
    /// The declaration that replaces the module in the root, as written up to its brace:
    /// comments, attributes and visibility, e.g. "#[cfg(test)]\nmod tests;"
    pub declaration: String,
    /// Use statements inside the module
    pub use_statements: Vec<String>,
    /// Splittable items (functions/methods) inside the module
    pub splittable_items: Vec<SplittableItem>,
    /// Non-function items inside the module (consts, types, etc.)
    pub mod_items: Vec<ModItem>,
}

/// Result of classifying a file's items.
pub struct ClassifiedFile {
    /// Source lines before the first item: inner attributes, module docs, comments
    pub preamble: String,
    /// Items that stay in mod.rs (structs, enums, traits, consts, use, etc.)
    pub mod_items: Vec<ModItem>,
    /// Items that get split into subfiles (functions, methods)
    pub splittable_items: Vec<SplittableItem>,
    /// Inline modules with splittable content (e.g. test modules)
    pub inline_modules: Vec<InlineModule>,
    /// Comments after the last item
    pub epilogue: Option<String>,
}

/// The parts of a fn declaration that decide its visibility edit.
struct FnParts<'a> {
    vis: &'a Visibility,
    defaultness: Option<&'a syn::token::Default>,
    sig: &'a syn::Signature,
}

/// The source of a syntax node with its common indent removed.
struct Dedented {
    text: String,
    /// Source line of the first row
    first_line: usize,
    /// Leading bytes removed from each row
    stripped: Vec<usize>,
    literal_rows: Vec<usize>,
}

/// Given a parsed file and its source, classify every top-level item.
pub fn classify_items(file: &syn::File, source: &str) -> Result<ClassifiedFile> {
    let lines: Vec<&str> = source.lines().collect();
    let mut mod_items = Vec::new();
    let mut splittable_items = Vec::new();
    let mut inline_modules = Vec::new();

    let first_item_line = file
        .items
        .iter()
        .filter_map(|item| token_lines(&item.to_token_stream()).map(|(start, _)| start))
        .min()
        .unwrap_or(lines.len() + 1);
    let preamble = lines[..first_item_line - 1].join("\n");

    let mut prev_end = first_item_line - 1;
    for item in &file.items {
        let (start, end) = token_lines(&item.to_token_stream()).context("item has no tokens")?;
        let from = leading_start(&lines, prev_end, start);
        prev_end = end;
        match item {
            Item::Fn(item_fn) => {
                let parts = FnParts {
                    vis: &item_fn.vis,
                    defaultness: None,
                    sig: &item_fn.sig,
                };
                let source = dedented_source(&lines, item, Some(from))?;
                splittable_items.push(moved_fn(source, parts, None)?);
            }
            Item::Impl(item_impl) => {
                classify_impl(
                    item_impl,
                    &lines,
                    Some(from),
                    &mut splittable_items,
                    &mut mod_items,
                )?;
            }
            Item::Mod(item_mod) => {
                // Check if this is an inline module with splittable content
                if let Some(inline) = try_classify_inline_module(item_mod, &lines, from)? {
                    inline_modules.push(inline);
                } else {
                    let text = extract_source(&lines, item, Some(from))?;
                    mod_items.push(ModItem { source_text: text });
                }
            }
            // Everything else stays in mod.rs
            _ => {
                let text = extract_source(&lines, item, Some(from))?;
                mod_items.push(ModItem { source_text: text });
            }
        }
    }
    let epilogue = comment_rows(&lines, prev_end + 1, lines.len())?;

    Ok(ClassifiedFile {
        preamble,
        mod_items,
        splittable_items,
        inline_modules,
        epilogue,
    })
}

/// Indent every row of `text` by four spaces, except blank rows and `literal_rows`.
pub fn indent_rows(text: &str, literal_rows: &[usize]) -> String {
    text.split('\n')
        .enumerate()
        .map(|(row, line)| {
            if line.trim().is_empty() || literal_rows.contains(&row) {
                line.to_string()
            } else {
                format!("    {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

mod fn_source;
mod impls;
mod inline_module;

use fn_source::comment_rows;
use fn_source::dedented_source;
use fn_source::extract_source;
use fn_source::leading_start;
use fn_source::moved_fn;
use fn_source::source_between;
use impls::classify_impl;
use inline_module::byte_offset;
use inline_module::collect_literal_rows;
use inline_module::leading_blanks;
use inline_module::token_lines;
use inline_module::try_classify_inline_module;
