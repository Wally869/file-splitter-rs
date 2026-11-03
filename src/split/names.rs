use std::cmp::Reverse;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use proc_macro2::{TokenStream, TokenTree};

use super::cluster::Cluster;

/// Rust keywords (strict and reserved, all editions): a part may not take one as its name.
const KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "crate",
    "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl",
    "in", "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub",
    "ref", "return", "self", "static", "struct", "super", "trait", "true", "try", "type",
    "typeof", "union", "unsafe", "unsized", "use", "virtual", "where", "while", "yield", "main",
    "lib", "tests",
];

/// Every identifier token of `tokens`, at any depth.
pub fn idents(tokens: TokenStream, out: &mut HashSet<String>) {
    for tree in tokens {
        match tree {
            TokenTree::Ident(ident) => {
                out.insert(ident.to_string().trim_start_matches("r#").to_string());
            }
            TokenTree::Group(group) => idents(group.stream(), out),
            _ => {}
        }
    }
}

/// The identifiers of `tokens` that a module of the same name would clash with: a path root
/// (`name::`), any name in a `use` item, and the name an item keyword declares in the type
/// namespace (`mod`, `struct`, `enum`, `union`, `trait`, `type`, `crate`).
pub fn module_clashes(tokens: TokenStream, out: &mut HashSet<String>) {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    let mut in_use = false;
    for (at, tree) in trees.iter().enumerate() {
        match tree {
            TokenTree::Group(group) if in_use => idents(group.stream(), out),
            TokenTree::Group(group) => module_clashes(group.stream(), out),
            TokenTree::Punct(p) if p.as_char() == ';' => in_use = false,
            TokenTree::Ident(ident) => {
                let name = ident.to_string().trim_start_matches("r#").to_string();
                if name == "use" {
                    in_use = true;
                    continue;
                }
                let path_root = matches!(
                    (trees.get(at + 1), trees.get(at + 2)),
                    (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b)))
                        if a.as_char() == ':' && b.as_char() == ':'
                );
                let declared = at > 0
                    && matches!(&trees[at - 1], TokenTree::Ident(kw) if matches!(
                        kw.to_string().as_str(),
                        "mod" | "struct" | "enum" | "union" | "trait" | "type" | "crate"
                    ));
                if in_use || path_root || declared {
                    out.insert(name);
                }
            }
            _ => {}
        }
    }
}

/// The names `tokens` declares in the value namespace: after `fn`, `const` and `static`.
pub fn value_declarations(tokens: TokenStream, out: &mut HashSet<String>) {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    for (at, tree) in trees.iter().enumerate() {
        match tree {
            TokenTree::Group(group) => value_declarations(group.stream(), out),
            TokenTree::Ident(ident) if at > 0 => {
                if matches!(&trees[at - 1], TokenTree::Ident(kw)
                    if matches!(kw.to_string().as_str(), "fn" | "const" | "static"))
                {
                    out.insert(ident.to_string().trim_start_matches("r#").to_string());
                }
            }
            _ => {}
        }
    }
}

/// The names of the `macro_rules!` macros of `tokens`, at its top level.
pub fn macro_rules_names(tokens: TokenStream) -> Vec<String> {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    trees
        .windows(3)
        .filter_map(|window| match window {
            [TokenTree::Ident(kw), TokenTree::Punct(bang), TokenTree::Ident(name)]
                if kw == "macro_rules" && bang.as_char() == '!' =>
            {
                Some(name.to_string())
            }
            _ => None,
        })
        .collect()
}

/// The lowercase words of an item name: split on `_`, spaces and case changes
/// (`Shape for Engine` gives `shape`, `for`, `engine`).
fn words(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut previous_lower = false;
    for c in name.chars() {
        if !c.is_ascii_alphanumeric() {
            words.push(std::mem::take(&mut current));
            previous_lower = false;
            continue;
        }
        if c.is_ascii_uppercase() && previous_lower {
            words.push(std::mem::take(&mut current));
        }
        previous_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        current.push(c.to_ascii_lowercase());
    }
    words.push(current);
    words.retain(|word| valid_name(word));
    words
}

fn valid_name(word: &str) -> bool {
    let mut chars = word.chars();
    word.len() >= 3
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && !KEYWORDS.contains(&word)
}

/// The words of a cluster's item names, best first: the word in the most item names, then
/// the one with the most lines, then the one nearest the start of a name (the prefix), then
/// in alphabetical order.
fn ranked_words(cluster: &Cluster) -> Vec<String> {
    let mut score: BTreeMap<String, (usize, usize, Reverse<usize>)> = BTreeMap::new();
    for item in &cluster.items {
        let mut seen = HashSet::new();
        for (position, word) in words(&item.name).into_iter().enumerate() {
            if seen.insert(word.clone()) {
                let entry = score.entry(word).or_insert((0, 0, Reverse(usize::MAX)));
                entry.0 += 1;
                entry.1 += item.line_count;
                entry.2 = entry.2.max(Reverse(position));
            }
        }
    }
    let mut words: Vec<(String, (usize, usize, Reverse<usize>))> = score.into_iter().collect();
    words.sort_by(|(a, sa), (b, sb)| sb.cmp(sa).then_with(|| a.cmp(b)));
    words.into_iter().map(|(word, _)| word).collect()
}

/// A module name for each cluster, from the words of its item names. A name is never a
/// keyword, the name of `dir`, a name of the source file a module would clash with (see
/// [`module_clashes`]), a file or folder already in `dir`, or the name of another part. When
/// the words of a cluster are all taken, its best word gets a number (`wrap_2`); a cluster
/// with no word gets `part_<n>`.
pub fn part_names(clusters: &[Cluster], dir: &Path, source_clashes: &HashSet<String>) -> Vec<String> {
    let mut taken: HashSet<String> = source_clashes.clone();
    if let Some(own) = dir.file_name().and_then(|name| name.to_str()) {
        taken.insert(own.to_string());
    }
    let free = |name: &str, taken: &HashSet<String>| {
        !taken.contains(name)
            && !dir.join(format!("{name}.rs")).exists()
            && !dir.join(name).exists()
    };
    let mut names = Vec::new();
    for (i, cluster) in clusters.iter().enumerate() {
        let words = ranked_words(cluster);
        let chosen = words
            .iter()
            .find(|word| free(word, &taken))
            .cloned()
            .or_else(|| {
                let base = words
                    .iter()
                    .find(|word| !source_clashes.contains(*word))
                    .or(words.first());
                base.and_then(|word| {
                    (2..10)
                        .map(|n| format!("{word}_{n}"))
                        .find(|name| free(name, &taken))
                })
            })
            .unwrap_or_else(|| {
                (i..)
                    .map(|n| format!("part_{n}"))
                    .find(|name| free(name, &taken))
                    .expect("a free part_<n> name exists")
            });
        taken.insert(chosen.clone());
        names.push(chosen);
    }
    names
}
