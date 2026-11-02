use super::parse::SplittableItem;
use std::collections::HashMap;

/// A cluster of related items that will become one subfile.
#[derive(Debug, Clone)]
pub struct Cluster {
    pub items: Vec<SplittableItem>,
    pub total_lines: usize,
}

/// Cluster splittable items using BM25 name similarity + agglomerative clustering.
pub fn cluster_items(items: &[SplittableItem], budget: usize) -> Vec<Cluster> {
    if items.is_empty() {
        return Vec::new();
    }

    // If everything fits in one file, just return a single cluster
    let total: usize = items.iter().map(|i| i.line_count).sum();
    if total <= budget {
        return vec![Cluster {
            items: items.to_vec(),
            total_lines: total,
        }];
    }

    // Tokenize all function names
    let tokenized: Vec<Vec<&str>> = items.iter().map(|i| tokenize(&i.name)).collect();

    // Build document frequency map (for BM25)
    let df = build_document_frequencies(&tokenized);
    let n = items.len() as f64;

    // Compute BM25 pairwise similarity matrix
    let similarity = compute_similarity_matrix(&tokenized, &df, n);

    // Agglomerative clustering with budget constraint
    let groups = agglomerative_cluster(items, &similarity, budget);
    let mut groups = merge_to_count(groups, &similarity, total.div_ceil(budget.max(1)), budget);
    groups.sort_by_key(|group| group.members.iter().min().copied());

    groups
        .into_iter()
        .map(|mut group| {
            group.members.sort_unstable();
            Cluster {
                items: group.members.iter().map(|&i| items[i].clone()).collect(),
                total_lines: group.lines,
            }
        })
        .collect()
}

/// Split `items` into the ones the root keeps and the ones that move to parts: the root keeps
/// the `pub` free functions, the module API, unless together they exceed `budget`.
pub fn split_root_items(
    items: &[SplittableItem],
    budget: usize,
) -> (Vec<SplittableItem>, Vec<SplittableItem>) {
    let (api, rest): (Vec<SplittableItem>, Vec<SplittableItem>) = items
        .iter()
        .cloned()
        .partition(|item| item.public && item.impl_context.is_none());
    let api_lines: usize = api.iter().map(|item| item.line_count).sum();
    if api_lines > budget {
        return (Vec::new(), items.to_vec());
    }
    (api, rest)
}

/// Item indices of one cluster under construction.
struct Group {
    members: Vec<usize>,
    lines: usize,
}

/// Average pairwise similarity between the members of two groups.
fn group_similarity(a: &Group, b: &Group, similarity: &[Vec<f64>]) -> f64 {
    let sum: f64 = a
        .members
        .iter()
        .flat_map(|&i| b.members.iter().map(move |&j| similarity[i][j]))
        .sum();
    sum / (a.members.len() * b.members.len()) as f64
}

/// Merge the smallest group into its nearest group until `target` groups are left: the
/// nearest that fits the budget, or, when none fits, the one that makes the smallest part.
fn merge_to_count(
    mut groups: Vec<Group>,
    similarity: &[Vec<f64>],
    target: usize,
    budget: usize,
) -> Vec<Group> {
    while groups.len() > target.max(1) {
        let Some(smallest) = (0..groups.len()).min_by_key(|&i| groups[i].lines) else {
            break;
        };
        let small = groups.swap_remove(smallest);
        let fits = |g: &Group| small.lines + g.lines <= budget;
        let into = (0..groups.len())
            .max_by(|&a, &b| {
                let (ga, gb) = (&groups[a], &groups[b]);
                fits(ga)
                    .cmp(&fits(gb))
                    .then_with(|| {
                        if fits(ga) {
                            group_similarity(&small, ga, similarity)
                                .total_cmp(&group_similarity(&small, gb, similarity))
                        } else {
                            std::cmp::Ordering::Equal
                        }
                    })
                    .then_with(|| gb.lines.cmp(&ga.lines))
            })
            .expect("more than one group remains before a merge");
        let group = &mut groups[into];
        group.members.extend(small.members);
        group.lines += small.lines;
    }
    groups
}

/// Split a function name into tokens on `_`.
fn tokenize(name: &str) -> Vec<&str> {
    name.split('_').filter(|s| !s.is_empty()).collect()
}

/// Count how many documents each token appears in.
fn build_document_frequencies<'a>(docs: &[Vec<&'a str>]) -> HashMap<&'a str, usize> {
    let mut df: HashMap<&str, usize> = HashMap::new();
    for doc in docs {
        // Count each unique token once per document
        let mut seen = std::collections::HashSet::new();
        for &token in doc {
            if seen.insert(token) {
                *df.entry(token).or_insert(0) += 1;
            }
        }
    }
    df
}

/// Compute BM25 similarity between two token lists.
/// We treat doc_a as the "query" and doc_b as the "document".
fn bm25_score(query: &[&str], doc: &[&str], df: &HashMap<&str, usize>, n: f64, avg_dl: f64) -> f64 {
    let k1: f64 = 1.2;
    let b: f64 = 0.75;
    let dl = doc.len() as f64;

    let mut score = 0.0;

    // Count term frequencies in doc
    let mut tf_map: HashMap<&str, usize> = HashMap::new();
    for &token in doc {
        *tf_map.entry(token).or_insert(0) += 1;
    }

    for &term in query {
        let tf = *tf_map.get(term).unwrap_or(&0) as f64;
        let doc_freq = *df.get(term).unwrap_or(&0) as f64;

        if tf == 0.0 || doc_freq == 0.0 {
            continue;
        }

        // IDF component
        let idf = ((n - doc_freq + 0.5) / (doc_freq + 0.5) + 1.0).ln();

        // TF component with length normalization
        let tf_norm = (tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * dl / avg_dl));

        score += idf * tf_norm;
    }

    score
}

/// Build a symmetric similarity matrix using BM25.
fn compute_similarity_matrix(
    docs: &[Vec<&str>],
    df: &HashMap<&str, usize>,
    n: f64,
) -> Vec<Vec<f64>> {
    let avg_dl = docs.iter().map(|d| d.len() as f64).sum::<f64>() / n.max(1.0);

    let len = docs.len();
    let mut sim = vec![vec![0.0f64; len]; len];

    for i in 0..len {
        for j in (i + 1)..len {
            // Symmetric: average of both directions
            let s1 = bm25_score(&docs[i], &docs[j], df, n, avg_dl);
            let s2 = bm25_score(&docs[j], &docs[i], df, n, avg_dl);
            let s = (s1 + s2) / 2.0;
            sim[i][j] = s;
            sim[j][i] = s;
        }
    }

    sim
}

/// Agglomerative clustering: start with each item as its own cluster,
/// merge most similar pairs until budget would be exceeded.
fn agglomerative_cluster(
    items: &[SplittableItem],
    similarity: &[Vec<f64>],
    budget: usize,
) -> Vec<Group> {
    let n = items.len();

    // Each element starts in its own cluster. Track cluster membership.
    let mut clusters: Vec<Option<Group>> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            Some(Group {
                members: vec![index],
                lines: item.line_count,
            })
        })
        .collect();

    // Compute inter-cluster similarity using average linkage.
    // We store a similarity matrix between clusters that updates as we merge.
    let mut cluster_sim: Vec<Vec<f64>> = similarity.to_vec();
    let mut active: Vec<bool> = vec![true; n];

    loop {
        // Find the most similar pair of active clusters
        let mut best_sim = f64::NEG_INFINITY;
        let mut best_i = 0;
        let mut best_j = 0;

        for i in 0..n {
            if !active[i] {
                continue;
            }
            for j in (i + 1)..n {
                if !active[j] {
                    continue;
                }
                if cluster_sim[i][j] > best_sim {
                    best_sim = cluster_sim[i][j];
                    best_i = i;
                    best_j = j;
                }
            }
        }

        // No more merges possible (or similarity is zero/negative)
        if best_sim <= 0.0 {
            break;
        }

        let (Some(ci), Some(cj)) = (&clusters[best_i], &clusters[best_j]) else {
            unreachable!("active clusters {best_i} and {best_j} are present");
        };

        // Check budget constraint
        if ci.lines + cj.lines > budget {
            // Can't merge these — mark this pair as unmergeable and try next
            cluster_sim[best_i][best_j] = f64::NEG_INFINITY;
            cluster_sim[best_j][best_i] = f64::NEG_INFINITY;
            continue;
        }

        // Merge j into i
        let (Some(mut merged), Some(cj_data)) = (clusters[best_i].take(), clusters[best_j].take())
        else {
            unreachable!("active clusters {best_i} and {best_j} are present");
        };
        merged.members.extend(cj_data.members);
        merged.lines += cj_data.lines;
        clusters[best_i] = Some(merged);
        active[best_j] = false;

        // Update similarity: average linkage between merged cluster and all others
        for k in 0..n {
            if !active[k] || k == best_i {
                continue;
            }
            let new_sim = (cluster_sim[best_i][k] + cluster_sim[best_j][k]) / 2.0;
            cluster_sim[best_i][k] = new_sim;
            cluster_sim[k][best_i] = new_sim;
        }
    }

    clusters.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, lines: usize, public: bool, method: bool) -> SplittableItem {
        SplittableItem {
            name: name.to_string(),
            source_text: String::new(),
            line_count: lines,
            impl_context: method.then(|| super::super::parse::ImplContext {
                header: "impl Foo".to_string(),
                self_type: "Foo".to_string(),
            }),
            trait_impl: false,
            public,
            root_vis: if public {
                "pub".to_string()
            } else {
                String::new()
            },
            vis_fix: None,
            literal_rows: Vec::new(),
        }
    }

    #[test]
    fn part_count_is_moved_lines_over_budget() {
        let names = [
            "parse_header",
            "parse_body",
            "emit_line",
            "emit_block",
            "check_rule",
            "check_set",
            "walk_tree",
            "walk_list",
            "load_file",
            "store_file",
        ];
        let items: Vec<SplittableItem> = names
            .iter()
            .map(|name| item(name, 30, false, true))
            .collect();

        let clusters = cluster_items(&items, 100);

        assert_eq!(clusters.len(), 3);
        let moved: usize = clusters.iter().map(|c| c.total_lines).sum();
        assert_eq!(moved, 300);
    }

    #[test]
    fn root_keeps_pub_free_functions() {
        let items = vec![
            item("reach", 40, true, false),
            item("reach_sources", 12, true, false),
            item("helper", 20, false, false),
            item("new", 30, true, true),
        ];

        let (kept, moved) = split_root_items(&items, 150);

        let kept: Vec<&str> = kept.iter().map(|i| i.name.as_str()).collect();
        let moved: Vec<&str> = moved.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(kept, ["reach", "reach_sources"]);
        assert_eq!(moved, ["helper", "new"]);
    }

    #[test]
    fn pub_free_functions_move_when_they_exceed_the_budget() {
        let items = vec![
            item("reach", 100, true, false),
            item("walk", 80, true, false),
        ];

        let (kept, moved) = split_root_items(&items, 150);

        assert!(kept.is_empty());
        assert_eq!(moved.len(), 2);
    }
}
