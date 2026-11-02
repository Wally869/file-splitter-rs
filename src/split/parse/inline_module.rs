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
    from_line: usize,
) -> Result<Option<InlineModule>> {
    // Only handle inline modules (with content), not external `mod foo;`
    let Some((brace, items)) = item_mod.content.as_ref() else {
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

    let open = brace.span.open().start();
    let head = source_between(
        lines,
        LineColumn {
            line: from_line,
            column: 0,
        },
        open,
    )?;
    let declaration = format!("{};", head.trim_end());

    let mut mod_use_statements = Vec::new();
    let mut mod_splittable = Vec::new();
    let mut mod_items = Vec::new();

    let mut prev_end = inner_attrs_end(&item_mod.attrs, open.line);
    let first_member = items
        .first()
        .and_then(|item| first_token_start(&item.to_token_stream()));
    let body_top = body_top(lines, brace, prev_end, first_member)?;
    for item in items {
        let (start, end) = token_lines(&item.to_token_stream()).context("item has no tokens")?;
        let from = Some(leading_start(lines, prev_end, start));
        prev_end = end;
        match item {
            Item::Fn(item_fn) => {
                let parts = FnParts {
                    vis: &item_fn.vis,
                    defaultness: None,
                    sig: &item_fn.sig,
                };
                let source = dedented_source(lines, item, from)?;
                mod_splittable.push(moved_fn(source, parts, None)?);
            }
            Item::Impl(item_impl) => {
                classify_impl(item_impl, lines, from, &mut mod_splittable, &mut mod_items)?;
            }
            Item::Use(_) => {
                mod_use_statements.push(dedented_source(lines, item, from)?.text);
            }
            _ => {
                let text = dedented_source(lines, item, from)?.text;
                mod_items.push(ModItem { source_text: text });
            }
        }
    }
    let close_line = brace.span.close().start().line;
    let epilogue = comment_rows(lines, prev_end + 1, close_line - 1)?;

    Ok(Some(InlineModule {
        name,
        declaration,
        body_top,
        use_statements: mod_use_statements,
        splittable_items: mod_splittable,
        mod_items,
        epilogue,
    }))
}

/// The last line of the inner attributes in `attrs`, or `brace_line` when that is later.
pub(super) fn inner_attrs_end(attrs: &[syn::Attribute], brace_line: usize) -> usize {
    attrs
        .iter()
        .filter(|attr| matches!(attr.style, syn::AttrStyle::Inner(_)))
        .map(|attr| attr.bracket_token.span.close().end().line)
        .fold(brace_line, usize::max)
}

pub(super) fn first_token_start(tokens: &TokenStream) -> Option<LineColumn> {
    tokens
        .clone()
        .into_iter()
        .next()
        .map(|tree| tree.span().start())
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
