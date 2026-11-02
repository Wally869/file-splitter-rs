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

/// A private item in a part must be `pub(in super::super)` to stay reachable from the root and the
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
            (at, at, "pub(in super::super) ")
        }
        Visibility::Restricted(r) if r.path.is_ident("self") => (
            r.pub_token.span.start(),
            r.paren_token.span.close().end(),
            "pub(in super::super)",
        ),
        _ => return Ok(None),
    };
    let range = byte_offset(source, start)?..byte_offset(source, end)?;
    Ok(Some(VisFix { range, replacement }))
}

/// The source lines of a syntax node, as written.
pub(super) fn extract_source(lines: &[&str], node: &impl ToTokens) -> Result<String> {
    let (start, end) = token_lines(&node.to_token_stream()).context("item has no tokens")?;
    let rows = lines
        .get(start - 1..end)
        .with_context(|| format!("lines {start}-{end} are outside the source"))?;
    Ok(rows.join("\n"))
}

/// The source lines of a syntax node, less the indent common to its code rows. A row that
/// starts inside a literal keeps its leading whitespace, which is literal content.
pub(super) fn dedented_source(lines: &[&str], node: &impl ToTokens) -> Result<Dedented> {
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
