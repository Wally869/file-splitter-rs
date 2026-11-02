use super::*;

/// Classify an impl block that starts with its leading comments on `from_line`. A trait impl
/// moves whole; an inherent impl gives its methods as splittable items and keeps its other
/// items in a partial impl in the root. Every block emitted for an inherent impl carries its
/// outer attributes.
pub(super) fn classify_impl(
    item_impl: &syn::ItemImpl,
    lines: &[&str],
    from_line: Option<usize>,
    splittable_items: &mut Vec<SplittableItem>,
    mod_items: &mut Vec<ModItem>,
) -> Result<()> {
    let self_ty = &item_impl.self_ty;
    let self_type = quote::quote!(#self_ty).to_string();

    if let Some((_, path, _)) = &item_impl.trait_ {
        let source = dedented_source(lines, item_impl, from_line)?;
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

    let (impl_start, _) =
        token_lines(&item_impl.to_token_stream()).context("impl has no tokens")?;
    let leading = match from_line {
        Some(from) => comment_rows(lines, from, impl_start - 1)?,
        None => None,
    };
    let header = format!(
        "{}{}",
        outer_attrs(lines, &item_impl.attrs)?,
        build_impl_header(item_impl)
    );

    // Inner attributes stay out of the first member's leading comments.
    let mut prev_end = item_impl
        .attrs
        .iter()
        .filter(|attr| matches!(attr.style, syn::AttrStyle::Inner(_)))
        .map(|attr| attr.bracket_token.span.close().end().line)
        .fold(item_impl.brace_token.span.open().start().line, usize::max);
    let mut kept = Vec::new();
    for impl_item in &item_impl.items {
        let (start, end) =
            token_lines(&impl_item.to_token_stream()).context("impl item has no tokens")?;
        let from = Some(leading_start(lines, prev_end, start));
        prev_end = end;
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
                let source = dedented_source(lines, method, from)?;
                splittable_items.push(moved_fn(source, parts, Some(context))?);
            }
            other => {
                let source = dedented_source(lines, other, from)?;
                kept.push(indent_rows(&source.text, &source.literal_rows));
            }
        }
    }
    let close_line = item_impl.brace_token.span.close().start().line;
    if let Some(trailing) = comment_rows(lines, prev_end + 1, close_line - 1)? {
        kept.push(indent_rows(&trailing, &[]));
    }

    let partial = (!kept.is_empty()).then(|| format!("{header} {{\n{}\n}}", kept.join("\n\n")));
    let root_text = match (leading, partial) {
        (Some(comments), Some(partial)) => Some(format!("{comments}\n{partial}")),
        (comments, partial) => comments.or(partial),
    };
    if let Some(source_text) = root_text {
        mod_items.push(ModItem { source_text });
    }
    Ok(())
}

/// The outer attributes of an item as written, one per row, each row ending in a newline.
fn outer_attrs(lines: &[&str], attrs: &[syn::Attribute]) -> Result<String> {
    let mut text = String::new();
    for attr in attrs
        .iter()
        .filter(|attr| matches!(attr.style, syn::AttrStyle::Outer))
    {
        let start = attr.pound_token.span.start();
        let end = attr.bracket_token.span.close().end();
        text.push_str(&source_between(lines, start, end)?);
        text.push('\n');
    }
    Ok(text)
}

/// Build the impl header string (e.g., `impl<T: Clone> Foo<T>` or `impl Trait for Foo`).
pub(super) fn build_impl_header(item_impl: &syn::ItemImpl) -> String {
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
