use super::*;

pub(super) fn byte_offset(source: &Dedented, at: LineColumn) -> Result<usize> {
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

/// Try to classify an inline module (e.g. `#[cfg(test)] mod tests { ... }`).
/// Returns `Some(InlineModule)` if the module has inline content with functions,
/// `None` otherwise (external `mod foo;` or no functions inside).
pub(super) fn try_classify_inline_module(
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
pub(super) fn token_lines(tokens: &TokenStream) -> Option<(usize, usize)> {
    let mut trees = tokens.clone().into_iter();
    let first = trees.next()?;
    let start = first.span().start().line;
    let end = trees.last().unwrap_or(first).span().end().line;
    Some((start, end))
}

pub(super) fn collect_literal_rows(tokens: TokenStream, first_line: usize, rows: &mut Vec<usize>) {
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

pub(super) fn leading_blanks(line: &str) -> usize {
    line.bytes()
        .take_while(|b| matches!(b, b' ' | b'\t'))
        .count()
}

/// Get the start line, considering attributes.
pub(super) fn attr_start_line(attrs: &[syn::Attribute], fallback: proc_macro2::Span) -> usize {
    if let Some(attr) = attrs.first() {
        attr.pound_token.span.start().line
    } else {
        fallback.start().line
    }
}
