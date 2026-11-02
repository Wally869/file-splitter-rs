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
    /// Attributes source text, e.g. "#[cfg(test)]"
    pub attrs_source: String,
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

    for item in &file.items {
        match item {
            Item::Fn(item_fn) => {
                let parts = FnParts {
                    vis: &item_fn.vis,
                    defaultness: None,
                    sig: &item_fn.sig,
                };
                splittable_items.push(moved_fn(dedented_source(&lines, item)?, parts, None)?);
            }
            Item::Impl(item_impl) => {
                classify_impl(item_impl, &lines, &mut splittable_items, &mut mod_items)?;
            }
            Item::Mod(item_mod) => {
                // Check if this is an inline module with splittable content
                if let Some(inline) = try_classify_inline_module(item_mod, &lines)? {
                    inline_modules.push(inline);
                } else {
                    let text = extract_source(&lines, item)?;
                    mod_items.push(ModItem { source_text: text });
                }
            }
            // Everything else stays in mod.rs
            _ => {
                let text = extract_source(&lines, item)?;
                mod_items.push(ModItem { source_text: text });
            }
        }
    }

    Ok(ClassifiedFile {
        preamble,
        mod_items,
        splittable_items,
        inline_modules,
    })
}

/// The parts of a fn declaration that decide its visibility edit.
struct FnParts<'a> {
    vis: &'a Visibility,
    defaultness: Option<&'a syn::token::Default>,
    sig: &'a syn::Signature,
}

/// Build a splittable item from the dedented source of a fn.
fn moved_fn(
    source: Dedented,
    parts: FnParts,
    impl_context: Option<ImplContext>,
) -> Result<SplittableItem> {
    let vis_fix = visibility_fix(&source, &parts)?;
    let root_vis = match parts.vis {
        Visibility::Inherited => String::new(),
        Visibility::Restricted(r) if r.path.is_ident("self") => String::new(),
        vis => quote::quote!(#vis).to_string(),
    };
    Ok(SplittableItem {
        name: parts.sig.ident.to_string(),
        line_count: source.text.lines().count(),
        source_text: source.text,
        impl_context,
        trait_impl: false,
        public: matches!(parts.vis, Visibility::Public(_)),
        root_vis,
        vis_fix,
        literal_rows: source.literal_rows,
    })
}

/// A private item in a part must be `pub(super)` to stay reachable from the root and the
/// other parts, which is the same reach it had as a private item of the root.
fn visibility_fix(source: &Dedented, parts: &FnParts) -> Result<Option<VisFix>> {
    let sig = parts.sig;
    let (start, end, replacement) = match parts.vis {
        Visibility::Inherited => {
            let at = parts
                .defaultness
                .map(|t| t.span)
                .or(sig.constness.map(|t| t.span))
                .or(sig.asyncness.map(|t| t.span))
                .or(sig.unsafety.map(|t| t.span))
                .or(sig.abi.as_ref().map(|abi| abi.extern_token.span))
                .unwrap_or(sig.fn_token.span)
                .start();
            (at, at, "pub(super) ")
        }
        Visibility::Restricted(r) if r.path.is_ident("self") => (
            r.pub_token.span.start(),
            r.paren_token.span.close().end(),
            "pub(super)",
        ),
        _ => return Ok(None),
    };
    let range = byte_offset(source, start)?..byte_offset(source, end)?;
    Ok(Some(VisFix { range, replacement }))
}

fn byte_offset(source: &Dedented, at: LineColumn) -> Result<usize> {
    let row = at
        .line
        .checked_sub(source.first_line)
        .with_context(|| format!("line {} is before the item start", at.line))?;
    let stripped = *source
        .stripped
        .get(row)
        .with_context(|| format!("line {} is past the end of the item", at.line))?;
    let column = at
        .column
        .checked_sub(stripped)
        .with_context(|| format!("column {} is inside the removed indent", at.column))?;
    let mut offset = 0;
    for (index, line) in source.text.split('\n').enumerate() {
        if index == row {
            let in_line = line
                .char_indices()
                .map(|(i, _)| i)
                .chain(std::iter::once(line.len()))
                .nth(column)
                .with_context(|| format!("column {column} is past the end of line {}", at.line))?;
            return Ok(offset + in_line);
        }
        offset += line.len() + 1;
    }
    anyhow::bail!("line {} is past the end of the item", at.line)
}

/// Classify an impl block. A trait impl moves whole; an inherent impl gives its methods as
/// splittable items and keeps its other items in a partial impl in the root.
fn classify_impl(
    item_impl: &syn::ItemImpl,
    lines: &[&str],
    splittable_items: &mut Vec<SplittableItem>,
    mod_items: &mut Vec<ModItem>,
) -> Result<()> {
    let self_ty = &item_impl.self_ty;
    let self_type = quote::quote!(#self_ty).to_string();

    if let Some((_, path, _)) = &item_impl.trait_ {
        let source = dedented_source(lines, item_impl)?;
        splittable_items.push(SplittableItem {
            name: format!("{} for {self_type}", quote::quote!(#path)),
            line_count: source.text.lines().count(),
            source_text: source.text,
            impl_context: None,
            trait_impl: true,
            public: false,
            root_vis: String::new(),
            vis_fix: None,
            literal_rows: source.literal_rows,
        });
        return Ok(());
    }

    let header = build_impl_header(item_impl);
    let mut kept = Vec::new();
    for impl_item in &item_impl.items {
        match impl_item {
            ImplItem::Fn(method) => {
                let parts = FnParts {
                    vis: &method.vis,
                    defaultness: method.defaultness.as_ref(),
                    sig: &method.sig,
                };
                let context = ImplContext {
                    header: header.clone(),
                    self_type: self_type.clone(),
                };
                let source = dedented_source(lines, method)?;
                splittable_items.push(moved_fn(source, parts, Some(context))?);
            }
            other => {
                let source = dedented_source(lines, other)?;
                kept.push(indent_rows(&source.text, &source.literal_rows));
            }
        }
    }

    if !kept.is_empty() {
        mod_items.push(ModItem {
            source_text: format!("{header} {{\n{}\n}}", kept.join("\n\n")),
        });
    }
    Ok(())
}

/// Try to classify an inline module (e.g. `#[cfg(test)] mod tests { ... }`).
/// Returns `Some(InlineModule)` if the module has inline content with functions,
/// `None` otherwise (external `mod foo;` or no functions inside).
fn try_classify_inline_module(
    item_mod: &syn::ItemMod,
    lines: &[&str],
) -> Result<Option<InlineModule>> {
    // Only handle inline modules (with content), not external `mod foo;`
    let Some((_brace, items)) = item_mod.content.as_ref() else {
        return Ok(None);
    };

    // Check if there are any functions inside
    let has_functions = items
        .iter()
        .any(|item| matches!(item, Item::Fn(_) | Item::Impl(_)));
    if !has_functions {
        return Ok(None);
    }

    let name = item_mod.ident.to_string();

    // Build attrs source text
    let attrs_source = if item_mod.attrs.is_empty() {
        String::new()
    } else {
        let attr_start = attr_start_line(&item_mod.attrs, item_mod.mod_token.span);
        let attr_end = item_mod.mod_token.span.start().line;
        let start_idx = attr_start.saturating_sub(1);
        let end_idx = attr_end.saturating_sub(1);
        if start_idx < lines.len() && end_idx <= lines.len() && start_idx < end_idx {
            lines[start_idx..end_idx].join("\n")
        } else {
            String::new()
        }
    };

    // Classify items inside the module
    let mut mod_use_statements = Vec::new();
    let mut mod_splittable = Vec::new();
    let mut mod_items = Vec::new();

    for item in items {
        match item {
            Item::Fn(item_fn) => {
                let parts = FnParts {
                    vis: &item_fn.vis,
                    defaultness: None,
                    sig: &item_fn.sig,
                };
                mod_splittable.push(moved_fn(dedented_source(lines, item)?, parts, None)?);
            }
            Item::Impl(item_impl) => {
                classify_impl(item_impl, lines, &mut mod_splittable, &mut mod_items)?;
            }
            Item::Use(_) => {
                mod_use_statements.push(dedented_source(lines, item)?.text);
            }
            _ => {
                let text = dedented_source(lines, item)?.text;
                mod_items.push(ModItem { source_text: text });
            }
        }
    }

    Ok(Some(InlineModule {
        name,
        attrs_source,
        use_statements: mod_use_statements,
        splittable_items: mod_splittable,
        mod_items,
    }))
}

/// First and last source line (1-indexed, inclusive) of a token stream; doc comments and
/// attributes are tokens, so they are included.
fn token_lines(tokens: &TokenStream) -> Option<(usize, usize)> {
    let mut trees = tokens.clone().into_iter();
    let first = trees.next()?;
    let start = first.span().start().line;
    let end = trees.last().unwrap_or(first).span().end().line;
    Some((start, end))
}

/// The source lines of a syntax node, as written.
fn extract_source(lines: &[&str], node: &impl ToTokens) -> Result<String> {
    let (start, end) = token_lines(&node.to_token_stream()).context("item has no tokens")?;
    let rows = lines
        .get(start - 1..end)
        .with_context(|| format!("lines {start}-{end} are outside the source"))?;
    Ok(rows.join("\n"))
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

/// The source lines of a syntax node, less the indent common to its code rows. A row that
/// starts inside a literal keeps its leading whitespace, which is literal content.
fn dedented_source(lines: &[&str], node: &impl ToTokens) -> Result<Dedented> {
    let tokens = node.to_token_stream();
    let (first_line, last_line) = token_lines(&tokens).context("item has no tokens")?;
    let rows = lines
        .get(first_line - 1..last_line)
        .with_context(|| format!("lines {first_line}-{last_line} are outside the source"))?;

    let mut literal_rows = Vec::new();
    collect_literal_rows(tokens, first_line, &mut literal_rows);
    literal_rows.sort_unstable();
    literal_rows.dedup();
    let is_literal = |row: usize| literal_rows.binary_search(&row).is_ok();

    let indent = rows
        .iter()
        .enumerate()
        .filter(|&(row, line)| !is_literal(row) && !line.trim().is_empty())
        .map(|(_, line)| leading_blanks(line))
        .min()
        .unwrap_or(0);
    let stripped: Vec<usize> = rows
        .iter()
        .enumerate()
        .map(|(row, line)| {
            if is_literal(row) {
                0
            } else {
                leading_blanks(line).min(indent)
            }
        })
        .collect();
    let text = rows
        .iter()
        .zip(&stripped)
        .map(|(line, &n)| &line[n..])
        .collect::<Vec<_>>()
        .join("\n");

    Ok(Dedented {
        text,
        first_line,
        stripped,
        literal_rows,
    })
}

fn collect_literal_rows(tokens: TokenStream, first_line: usize, rows: &mut Vec<usize>) {
    for tree in tokens {
        match tree {
            TokenTree::Group(group) => collect_literal_rows(group.stream(), first_line, rows),
            TokenTree::Literal(literal) => {
                let span = literal.span();
                rows.extend(
                    (span.start().line + 1..=span.end().line).map(|line| line - first_line),
                );
            }
            TokenTree::Ident(_) | TokenTree::Punct(_) => {}
        }
    }
}

fn leading_blanks(line: &str) -> usize {
    line.bytes()
        .take_while(|b| matches!(b, b' ' | b'\t'))
        .count()
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

/// Get the start line, considering attributes.
fn attr_start_line(attrs: &[syn::Attribute], fallback: proc_macro2::Span) -> usize {
    if let Some(attr) = attrs.first() {
        attr.pound_token.span.start().line
    } else {
        fallback.start().line
    }
}

/// Build the impl header string (e.g., `impl<T: Clone> Foo<T>` or `impl Trait for Foo`).
fn build_impl_header(item_impl: &syn::ItemImpl) -> String {
    let unsafety = item_impl.unsafety.map(|_| "unsafe ").unwrap_or("");

    let generics = if item_impl.generics.params.is_empty() {
        String::new()
    } else {
        let params = &item_impl.generics;
        quote::quote!(#params).to_string()
    };

    let trait_part = if let Some((bang, path, _for_token)) = &item_impl.trait_ {
        let bang = if bang.is_some() { "! " } else { "" };
        let path = quote::quote!(#path).to_string();
        format!("{bang}{path} for ")
    } else {
        String::new()
    };

    let self_ty = &item_impl.self_ty;
    let self_ty_str = quote::quote!(#self_ty).to_string();

    let where_clause = item_impl
        .generics
        .where_clause
        .as_ref()
        .map(|w| format!(" {}", quote::quote!(#w)))
        .unwrap_or_default();

    format!("{unsafety}impl{generics} {trait_part}{self_ty_str}{where_clause}")
}
