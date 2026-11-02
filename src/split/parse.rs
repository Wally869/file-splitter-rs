use syn::{Item, ImplItem};

/// A single splittable item (function or method) with its source text.
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
    /// Items that stay in mod.rs (structs, enums, traits, consts, use, etc.)
    pub mod_items: Vec<ModItem>,
    /// Items that get split into subfiles (functions, methods)
    pub splittable_items: Vec<SplittableItem>,
    /// Use statements from the original file (copied to every subfile)
    pub use_statements: Vec<String>,
    /// Inline modules with splittable content (e.g. test modules)
    pub inline_modules: Vec<InlineModule>,
}

/// Given a parsed file and its source, classify every top-level item.
pub fn classify_items(file: &syn::File, source: &str) -> ClassifiedFile {
    let lines: Vec<&str> = source.lines().collect();
    let mut mod_items = Vec::new();
    let mut splittable_items = Vec::new();
    let mut use_statements = Vec::new();
    let mut inline_modules = Vec::new();

    for item in &file.items {
        match item {
            Item::Fn(item_fn) => {
                let text = extract_item_source(&lines, item);
                let line_count = text.lines().count();
                splittable_items.push(SplittableItem {
                    name: item_fn.sig.ident.to_string(),
                    source_text: text,
                    line_count,
                    impl_context: None,
                });
            }
            Item::Impl(item_impl) => {
                classify_impl(item_impl, &lines, &mut splittable_items, &mut mod_items);
            }
            Item::Use(_) => {
                let text = extract_item_source(&lines, item);
                use_statements.push(text.clone());
                mod_items.push(ModItem { source_text: text });
            }
            Item::Mod(item_mod) => {
                // Check if this is an inline module with splittable content
                if let Some(inline) = try_classify_inline_module(item_mod, &lines) {
                    inline_modules.push(inline);
                } else {
                    let text = extract_item_source(&lines, item);
                    mod_items.push(ModItem { source_text: text });
                }
            }
            // Everything else stays in mod.rs
            _ => {
                let text = extract_item_source(&lines, item);
                mod_items.push(ModItem { source_text: text });
            }
        }
    }

    ClassifiedFile {
        mod_items,
        splittable_items,
        use_statements,
        inline_modules,
    }
}

/// Classify an impl block: extract methods as splittable, keep non-method items in mod.
fn classify_impl(
    item_impl: &syn::ItemImpl,
    lines: &[&str],
    splittable_items: &mut Vec<SplittableItem>,
    mod_items: &mut Vec<ModItem>,
) {
    let self_ty = &item_impl.self_ty;
    let self_type = quote::quote!(#self_ty).to_string();
    let header = build_impl_header(item_impl);

    let mut has_non_methods = false;
    let mut methods = Vec::new();

    for impl_item in &item_impl.items {
        match impl_item {
            ImplItem::Fn(method) => {
                let text = extract_impl_item_source(lines, impl_item);
                let line_count = text.lines().count();
                methods.push(SplittableItem {
                    name: method.sig.ident.to_string(),
                    source_text: text,
                    line_count,
                    impl_context: Some(ImplContext {
                        header: header.clone(),
                        self_type: self_type.clone(),
                    }),
                });
            }
            _ => {
                has_non_methods = true;
            }
        }
    }

    if has_non_methods {
        let partial = build_partial_impl_source(item_impl, lines);
        mod_items.push(ModItem {
            source_text: partial,
        });
    }

    splittable_items.extend(methods);
}

/// Try to classify an inline module (e.g. `#[cfg(test)] mod tests { ... }`).
/// Returns `Some(InlineModule)` if the module has inline content with functions,
/// `None` otherwise (external `mod foo;` or no functions inside).
fn try_classify_inline_module(
    item_mod: &syn::ItemMod,
    lines: &[&str],
) -> Option<InlineModule> {
    // Only handle inline modules (with content), not external `mod foo;`
    let (_brace, items) = item_mod.content.as_ref()?;

    // Check if there are any functions inside
    let has_functions = items.iter().any(|item| matches!(
        item,
        Item::Fn(_) | Item::Impl(_)
    ));
    if !has_functions {
        return None;
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
                let text = dedent(&extract_item_source(lines, item));
                let line_count = text.lines().count();
                mod_splittable.push(SplittableItem {
                    name: item_fn.sig.ident.to_string(),
                    source_text: text,
                    line_count,
                    impl_context: None,
                });
            }
            Item::Impl(item_impl) => {
                classify_impl(item_impl, lines, &mut mod_splittable, &mut mod_items);
            }
            Item::Use(_) => {
                let text = dedent(&extract_item_source(lines, item));
                mod_use_statements.push(text);
            }
            _ => {
                let text = dedent(&extract_item_source(lines, item));
                if !text.is_empty() {
                    mod_items.push(ModItem { source_text: text });
                }
            }
        }
    }

    Some(InlineModule {
        name,
        attrs_source,
        use_statements: mod_use_statements,
        splittable_items: mod_splittable,
        mod_items,
    })
}

/// Extract the source text for a top-level item, including leading attributes/docs.
fn extract_item_source(lines: &[&str], item: &Item) -> String {
    let (start_line, end_line) = item_span(item);
    if start_line == 0 || end_line == 0 || start_line > lines.len() {
        return String::new();
    }
    let start_idx = start_line.saturating_sub(1);
    let end_idx = end_line.min(lines.len());
    lines[start_idx..end_idx].join("\n")
}

/// Get (start_line, end_line) for a top-level item (1-indexed, inclusive).
fn item_span(item: &Item) -> (usize, usize) {
    match item {
        Item::Fn(f) => (
            attr_start_line(&f.attrs, f.sig.fn_token.span),
            f.block.brace_token.span.close().end().line,
        ),
        Item::Struct(s) => (
            attr_start_line(&s.attrs, s.struct_token.span),
            s.semi_token
                .map(|t| t.span.end().line)
                .or(s.fields.span_close_line())
                .unwrap_or(s.struct_token.span.end().line),
        ),
        Item::Enum(e) => (
            attr_start_line(&e.attrs, e.enum_token.span),
            e.brace_token.span.close().end().line,
        ),
        Item::Trait(t) => (
            attr_start_line(&t.attrs, t.trait_token.span),
            t.brace_token.span.close().end().line,
        ),
        Item::Impl(i) => (
            attr_start_line(&i.attrs, i.impl_token.span),
            i.brace_token.span.close().end().line,
        ),
        Item::Use(u) => (
            attr_start_line(&u.attrs, u.use_token.span),
            u.semi_token.span.end().line,
        ),
        Item::Const(c) => (
            attr_start_line(&c.attrs, c.const_token.span),
            c.semi_token.span.end().line,
        ),
        Item::Static(s) => (
            attr_start_line(&s.attrs, s.static_token.span),
            s.semi_token.span.end().line,
        ),
        Item::Type(t) => (
            attr_start_line(&t.attrs, t.type_token.span),
            t.semi_token.span.end().line,
        ),
        Item::Mod(m) => (
            attr_start_line(&m.attrs, m.mod_token.span),
            m.semi.map(|t| t.span.end().line).unwrap_or_else(|| {
                m.content
                    .as_ref()
                    .map(|(brace, _)| brace.span.close().end().line)
                    .unwrap_or(m.mod_token.span.end().line)
            }),
        ),
        Item::Macro(m) => {
            let kw_span = m.mac.path.segments.first().map_or_else(
                || m.mac.bang_token.span,
                |seg| seg.ident.span(),
            );
            let end = m.semi_token
                .map(|t| t.span.end().line)
                .unwrap_or_else(|| m.mac.delimiter.span().close().end().line);
            (attr_start_line(&m.attrs, kw_span), end)
        }
        _ => (0, 0),
    }
}

/// Get the start line, considering attributes.
fn attr_start_line(attrs: &[syn::Attribute], fallback: proc_macro2::Span) -> usize {
    if let Some(attr) = attrs.first() {
        attr.pound_token.span.start().line
    } else {
        fallback.start().line
    }
}

/// Helper trait to get closing brace line from struct fields.
trait SpanCloseLine {
    fn span_close_line(&self) -> Option<usize>;
}

impl SpanCloseLine for syn::Fields {
    fn span_close_line(&self) -> Option<usize> {
        match self {
            syn::Fields::Named(f) => Some(f.brace_token.span.close().end().line),
            syn::Fields::Unnamed(f) => Some(f.paren_token.span.close().end().line),
            syn::Fields::Unit => None,
        }
    }
}

/// Strip common leading whitespace from a block of text.
fn dedent(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);

    if min_indent == 0 {
        return text.to_string();
    }

    lines
        .iter()
        .map(|l| {
            if l.len() >= min_indent {
                &l[min_indent..]
            } else {
                l.trim()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract source for an impl item (method, const, type within impl block).
fn extract_impl_item_source(lines: &[&str], impl_item: &ImplItem) -> String {
    match impl_item {
        ImplItem::Fn(f) => {
            let start = if let Some(attr) = f.attrs.first() {
                attr.pound_token.span.start().line
            } else {
                f.sig.fn_token.span.start().line
            };
            let end = f.block.brace_token.span.close().end().line;

            let start_idx = start.saturating_sub(1);
            let end_idx = end.min(lines.len());
            lines[start_idx..end_idx].join("\n")
        }
        _ => String::new(),
    }
}

/// Build the impl header string (e.g., `impl<T: Clone> Foo<T>` or `impl Trait for Foo`).
fn build_impl_header(item_impl: &syn::ItemImpl) -> String {
    let unsafety = item_impl
        .unsafety
        .map(|_| "unsafe ")
        .unwrap_or("");

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

/// Build source for a partial impl block (non-method items only).
fn build_partial_impl_source(item_impl: &syn::ItemImpl, lines: &[&str]) -> String {
    let header = build_impl_header(item_impl);
    let mut parts = vec![format!("{header} {{")];

    for impl_item in &item_impl.items {
        match impl_item {
            ImplItem::Fn(_) => {} // skip methods, they get split
            _ => {
                let text = extract_impl_item_source(lines, impl_item);
                if !text.is_empty() {
                    parts.push(format!("    {text}"));
                }
            }
        }
    }

    parts.push("}".to_string());
    parts.join("\n")
}
