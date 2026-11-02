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
    agglomerative_cluster(items, &similarity, budget)
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
fn bm25_score(
    query: &[&str],
    doc: &[&str],
    df: &HashMap<&str, usize>,
    n: f64,
    avg_dl: f64,
) -> f64 {
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
) -> Vec<Cluster> {
    let n = items.len();

    // Each element starts in its own cluster. Track cluster membership.
    let mut clusters: Vec<Option<Cluster>> = items
        .iter()
        .map(|item| {
            Some(Cluster {
                total_lines: item.line_count,
                items: vec![item.clone()],
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

        let ci = clusters[best_i].as_ref().unwrap();
        let cj = clusters[best_j].as_ref().unwrap();

        // Check budget constraint
        if ci.total_lines + cj.total_lines > budget {
            // Can't merge these — mark this pair as unmergeable and try next
            cluster_sim[best_i][best_j] = f64::NEG_INFINITY;
            cluster_sim[best_j][best_i] = f64::NEG_INFINITY;
            continue;
        }

        // Merge j into i
        let mut merged_items = clusters[best_i].take().unwrap().items;
        let cj_data = clusters[best_j].take().unwrap();
        merged_items.extend(cj_data.items);
        let merged_lines: usize = merged_items.iter().map(|item| item.line_count).sum();

        clusters[best_i] = Some(Cluster {
            items: merged_items,
            total_lines: merged_lines,
        });
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

    // Collect remaining clusters
    let result: Vec<Cluster> = clusters.into_iter().flatten().collect();

    // Merge tiny orphan clusters (< 10 lines) into a misc bucket
    let min_size = 10;
    let mut orphans = Vec::new();
    let mut kept = Vec::new();

    for cluster in result {
        if cluster.total_lines < min_size && cluster.items.len() == 1 {
            orphans.push(cluster);
        } else {
            kept.push(cluster);
        }
    }

    if !orphans.is_empty() {
        let mut misc_items = Vec::new();
        let mut misc_lines = 0;
        for orphan in orphans {
            misc_lines += orphan.total_lines;
            misc_items.extend(orphan.items);
        }
        kept.push(Cluster {
            items: misc_items,
            total_lines: misc_lines,
        });
    }

    kept
}
