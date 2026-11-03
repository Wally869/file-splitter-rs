use super::*;

/// Build a splittable item from the dedented source of a fn.
pub(super) fn moved_fn(
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
pub(super) fn visibility_fix(source: &Dedented, parts: &FnParts) -> Result<Option<VisFix>> {
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

/// The source lines of a syntax node, as written, from `from_line` when it comes first.
pub(super) fn extract_source(
    lines: &[&str],
    node: &impl ToTokens,
    from_line: Option<usize>,
) -> Result<String> {
    let (start, end) = token_lines(&node.to_token_stream()).context("item has no tokens")?;
    let start = from_line.map_or(start, |from| from.min(start));
    let rows = lines
        .get(start - 1..end)
        .with_context(|| format!("lines {start}-{end} are outside the source"))?;
    Ok(rows.join("\n"))
}

/// The first line of the comments before an item that starts on `start`: the first
/// non-blank line after `prev_end`, the last line of the item before it.
pub(super) fn leading_start(lines: &[&str], prev_end: usize, start: usize) -> usize {
    (prev_end + 1..start)
        .find(|&line| {
            lines
                .get(line - 1)
                .is_some_and(|row| !row.trim().is_empty())
        })
        .unwrap_or(start)
}

/// Lines `first` to `last` (1-indexed, inclusive) less their outer blank lines and common
/// indent, or `None` when they are all blank.
pub(super) fn comment_rows(lines: &[&str], first: usize, last: usize) -> Result<Option<String>> {
    if first > last {
        return Ok(None);
    }
    let rows = lines
        .get(first - 1..last)
        .with_context(|| format!("lines {first}-{last} are outside the source"))?;
    let Some(top) = rows.iter().position(|row| !row.trim().is_empty()) else {
        return Ok(None);
    };
    let bottom = rows
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .context("a non-blank row exists")?;
    let rows = &rows[top..=bottom];
    let indent = rows
        .iter()
        .filter(|row| !row.trim().is_empty())
        .map(|row| leading_blanks(row))
        .min()
        .unwrap_or(0);
    let text = rows
        .iter()
        .map(|row| &row[leading_blanks(row).min(indent)..])
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Some(text))
}

/// The top of a block body as written: from after its `{` to the end of line `prev_end` (the
/// last line of its inner attributes, or the brace line), stopping at the first member when
/// that starts earlier. Comments and inner attributes, less outer blank rows and the indent of
/// the rows after the first, or `None` when blank.
pub(super) fn body_top(
    lines: &[&str],
    brace: &syn::token::Brace,
    prev_end: usize,
    first_member: Option<LineColumn>,
) -> Result<Option<String>> {
    let open = brace.span.open().end();
    let stop = [
        Some(LineColumn {
            line: prev_end,
            column: usize::MAX,
        }),
        Some(brace.span.close().start()),
        first_member,
    ]
    .into_iter()
    .flatten()
    .min()
    .context("a stop position exists")?;
    let text = source_between(lines, open, stop)?;
    let rows: Vec<&str> = text.split('\n').collect();
    let indent = rows[1..]
        .iter()
        .filter(|row| !row.trim().is_empty())
        .map(|row| leading_blanks(row))
        .min()
        .unwrap_or(0);
    let rows: Vec<&str> = std::iter::once(rows[0].trim_start())
        .chain(
            rows[1..]
                .iter()
                .map(|row| &row[leading_blanks(row).min(indent)..]),
        )
        .collect();
    let Some(top) = rows.iter().position(|row| !row.trim().is_empty()) else {
        return Ok(None);
    };
    let bottom = rows
        .iter()
        .rposition(|row| !row.trim().is_empty())
        .context("a non-blank row exists")?;
    Ok(Some(rows[top..=bottom].join("\n").trim_end().to_string()))
}

/// The source text from `start` to `end`.
pub(super) fn source_between(lines: &[&str], start: LineColumn, end: LineColumn) -> Result<String> {
    let rows = lines
        .get(start.line - 1..end.line)
        .with_context(|| format!("lines {}-{} are outside the source", start.line, end.line))?;
    let mut text = String::new();
    for (index, row) in rows.iter().enumerate() {
        let line = start.line + index;
        let from = if line == start.line { start.column } else { 0 };
        let to = if line == end.line {
            end.column
        } else {
            usize::MAX
        };
        if index > 0 {
            text.push('\n');
        }
        text.extend(row.chars().skip(from).take(to.saturating_sub(from)));
    }
    Ok(text)
}

/// The source lines of a syntax node from `from_line` when it comes first, less the indent
/// common to its code rows. A row that starts inside a literal keeps its leading
/// whitespace, which is literal content.
pub(super) fn dedented_source(
    lines: &[&str],
    node: &impl ToTokens,
    from_line: Option<usize>,
) -> Result<Dedented> {
    let tokens = node.to_token_stream();
    let (first_line, last_line) = token_lines(&tokens).context("item has no tokens")?;
    let first_line = from_line.map_or(first_line, |from| from.min(first_line));
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
