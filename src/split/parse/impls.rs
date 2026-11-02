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
    let header = impl_header(lines, item_impl)?;

    // Inner attributes stay out of the first member's leading comments.
    let mut prev_end = inner_attrs_end(
        &item_impl.attrs,
        item_impl.brace_token.span.open().start().line,
    );
    let first_member = item_impl
        .items
        .first()
        .and_then(|member| first_token_start(&member.to_token_stream()));
    let body_top = body_top(lines, &item_impl.brace_token, prev_end, first_member)?;
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
                    body_top: body_top.clone(),
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

    if let (false, Some(top)) = (kept.is_empty(), &body_top) {
        kept.insert(0, indent_rows(top, &[]));
    }
    let partial = (!kept.is_empty()).then(|| format!("{header}{{\n{}\n}}", kept.join("\n\n")));
    let root_text = match (leading, partial) {
        (Some(comments), Some(partial)) => Some(format!("{comments}\n{partial}")),
        (comments, partial) => comments.or(partial),
    };
    if let Some(source_text) = root_text {
        mod_items.push(ModItem { source_text });
    }
    Ok(())
}

/// The header of an impl as written, from its first outer attribute (or its first keyword)
/// up to its `{`, with the comments in it and the whitespace before the `{`, less the impl's
/// indent. Followed by `{`, it opens the impl as in the source.
fn impl_header(lines: &[&str], item_impl: &syn::ItemImpl) -> Result<String> {
    let start = item_impl
        .attrs
        .iter()
        .find(|attr| matches!(attr.style, syn::AttrStyle::Outer))
        .map(|attr| attr.pound_token.span)
        .or(item_impl.defaultness.map(|token| token.span))
        .or(item_impl.unsafety.map(|token| token.span))
        .unwrap_or(item_impl.impl_token.span)
        .start();
    let end = item_impl.brace_token.span.open().start();
    let text = source_between(lines, start, end)?;
    let rows: Vec<&str> = text
        .split('\n')
        .enumerate()
        .map(|(row, line)| {
            if row == 0 {
                line
            } else {
                &line[leading_blanks(line).min(start.column)..]
            }
        })
        .collect();
    Ok(rows.join("\n"))
}
