/// Embedding-only Graph RAG Search
/// Layer 1: Cosine similarity between query embedding and node embeddings → entry points
/// Layer 2: Graph walk (1-2 hop BFS) → candidate expansion
/// Layer 3: MMR reranking → diverse final results
/// Draft nodes are ALWAYS excluded from search results.

use std::collections::HashMap;
use uuid::Uuid;

use crate::embedding::cosine_similarity;
use crate::graph::AmureGraph;
use crate::node::NodeStatus;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub node_id: Uuid,
    pub statement: String,
    pub abstract_: String,
    pub score: f64,
    pub hop_distance: usize,
    pub path: Vec<Uuid>,
    pub status: String,
    pub n_experiments: usize,
}

/// 검색 옵션
pub struct SearchOptions {
    pub top_k: usize,
    pub max_hops: usize,
    pub include_decline: bool,
    pub mmr_lambda: f64,
    /// 특정 상태만 반환. None이면 전체 (Draft 제외).
    pub status_filter: Option<String>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            top_k: 10,
            max_hops: 2,
            include_decline: false,
            mmr_lambda: 0.7,
            status_filter: None,
        }
    }
}

/// Embedding-based search. Returns empty if query_embedding is None.
pub fn search(
    graph: &AmureGraph,
    opts: &SearchOptions,
    query_embedding: Option<&[f32]>,
) -> Vec<SearchResult> {
    // No embedding → no results (no keyword fallback)
    let q_emb = match query_embedding {
        Some(emb) => emb,
        None => return Vec::new(),
    };

    // Step 1: Cosine similarity against all non-Draft nodes
    let mut emb_scored: Vec<(Uuid, f64)> = graph.nodes.iter()
        .filter(|(_, node)| node.status != NodeStatus::Draft)
        .filter(|(_, node)| opts.include_decline || node.status != NodeStatus::Decline)
        .filter_map(|(id, node)| {
            let emb = node.embedding.as_ref()?;
            let sim = cosine_similarity(q_emb, emb);
            if sim > 0.1 { Some((*id, sim)) } else { None }
        })
        .collect();
    emb_scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    emb_scored.truncate(opts.top_k * 3);

    // Step 2: Graph walk from entry points
    let candidates = graph_walk(graph, &emb_scored, opts.max_hops);

    // Step 3: Assign embedding scores to all candidates
    let mut final_candidates: HashMap<Uuid, (f64, usize, Vec<Uuid>)> = HashMap::new();
    for (id, (_walk_score, hop, path)) in &candidates {
        let node = match graph.get_node(id) {
            Some(n) => n,
            None => continue,
        };
        // Always exclude Draft
        if node.status == NodeStatus::Draft { continue; }
        // Exclude Decline unless requested
        if !opts.include_decline && node.status == NodeStatus::Decline { continue; }

        let emb_score = node.embedding.as_ref()
            .map(|emb| cosine_similarity(q_emb, emb))
            .unwrap_or(0.0);
        final_candidates.insert(*id, (emb_score, *hop, path.clone()));
    }

    // Step 4: MMR reranking
    let mut results = mmr_rerank(graph, final_candidates, opts);

    // Apply status filter
    if let Some(ref status) = opts.status_filter {
        results.retain(|r| r.status.eq_ignore_ascii_case(status));
    }

    results.truncate(opts.top_k);
    results
}

/// Balanced search: return n Accept + n Decline results
pub fn search_balanced(
    graph: &AmureGraph,
    n: usize,
    query_embedding: Option<&[f32]>,
) -> Vec<SearchResult> {
    let q_emb = match query_embedding {
        Some(emb) => emb,
        None => return Vec::new(),
    };

    // Get Accept results
    let accept_opts = SearchOptions {
        top_k: n,
        include_decline: false,
        status_filter: Some("Accept".to_string()),
        ..Default::default()
    };
    let accept_results = search(graph, &accept_opts, Some(q_emb));

    // Get Decline results
    let decline_opts = SearchOptions {
        top_k: n,
        include_decline: true,
        status_filter: Some("Decline".to_string()),
        ..Default::default()
    };
    let decline_results = search(graph, &decline_opts, Some(q_emb));

    let mut results = Vec::new();
    results.extend(accept_results);
    results.extend(decline_results);
    results
}

/// Layer 2: Graph walk from entry points, collecting candidates with decayed scores
fn graph_walk(
    graph: &AmureGraph,
    entry_points: &[(Uuid, f64)],
    max_hops: usize,
) -> HashMap<Uuid, (f64, usize, Vec<Uuid>)> {
    let mut candidates: HashMap<Uuid, (f64, usize, Vec<Uuid>)> = HashMap::new();

    for (entry_id, entry_score) in entry_points {
        let walked = graph.walk(entry_id, max_hops, None);
        for (node_id, hop) in walked {
            let decayed_score = entry_score * 0.5f64.powi(hop as i32);
            let path = vec![*entry_id, node_id];

            candidates.entry(node_id)
                .and_modify(|(s, h, p)| {
                    if decayed_score > *s {
                        *s = decayed_score;
                        *h = hop;
                        *p = path.clone();
                    }
                })
                .or_insert((decayed_score, hop, path));
        }
    }

    candidates
}

/// Layer 3: MMR (Maximal Marginal Relevance) reranking using embedding similarity
fn mmr_rerank(
    graph: &AmureGraph,
    candidates: HashMap<Uuid, (f64, usize, Vec<Uuid>)>,
    opts: &SearchOptions,
) -> Vec<SearchResult> {
    if candidates.is_empty() {
        return Vec::new();
    }

    let lambda = opts.mmr_lambda;
    let mut remaining: Vec<(Uuid, f64, usize, Vec<Uuid>)> = candidates
        .into_iter()
        .map(|(id, (score, hop, path))| (id, score, hop, path))
        .collect();

    let mut selected: Vec<SearchResult> = Vec::new();
    let mut selected_embeddings: Vec<Vec<f32>> = Vec::new();

    while !remaining.is_empty() && selected.len() < opts.top_k {
        let mut best_idx = 0;
        let mut best_mmr = f64::NEG_INFINITY;

        for (i, (id, score, _, _)) in remaining.iter().enumerate() {
            let relevance = *score;

            // Max embedding similarity to already selected
            let max_sim = if selected_embeddings.is_empty() {
                0.0
            } else {
                let node_emb = graph.get_node(id)
                    .and_then(|n| n.embedding.as_ref());
                match node_emb {
                    Some(emb) => selected_embeddings.iter()
                        .map(|sel_emb| cosine_similarity(emb, sel_emb))
                        .fold(0.0f64, f64::max),
                    None => 0.0,
                }
            };

            let mmr = lambda * relevance - (1.0 - lambda) * max_sim;
            if mmr > best_mmr {
                best_mmr = mmr;
                best_idx = i;
            }
        }

        let (id, score, hop, path) = remaining.remove(best_idx);
        if let Some(node) = graph.get_node(&id) {
            if let Some(emb) = node.embedding.as_ref() {
                selected_embeddings.push(emb.clone());
            }

            selected.push(SearchResult {
                node_id: id,
                statement: node.statement.clone(),
                abstract_: node.abstract_.clone(),
                score,
                hop_distance: hop,
                path,
                status: format!("{:?}", node.status),
                n_experiments: node.experiments.len(),
            });
        }
    }

    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Node;

    fn make_node_with_embedding(statement: &str, emb: Vec<f32>, status: NodeStatus) -> Node {
        let mut n = Node::new(statement.into());
        n.status = status;
        n.embedding = Some(emb);
        n
    }

    fn build_test_graph() -> AmureGraph {
        let mut g = AmureGraph::new();

        g.add_node(make_node_with_embedding(
            "OI 변화량은 momentum의 선행지표다",
            vec![1.0, 0.0, 0.0],
            NodeStatus::Accept,
        ));

        g.add_node(make_node_with_embedding(
            "funding rate 극단값은 mean reversion 시그널이다",
            vec![0.0, 1.0, 0.0],
            NodeStatus::Accept,
        ));

        g.add_node(make_node_with_embedding(
            "OI 감소는 디레버리징의 시그널이다",
            vec![0.9, 0.1, 0.0],
            NodeStatus::Decline,
        ));

        // Draft node — should never appear in results
        g.add_node(make_node_with_embedding(
            "Draft hypothesis",
            vec![1.0, 0.0, 0.0],
            NodeStatus::Draft,
        ));

        g
    }

    #[test]
    fn test_search_returns_empty_without_embedding() {
        let g = build_test_graph();
        let results = search(&g, &SearchOptions::default(), None);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_basic() {
        let g = build_test_graph();
        let query_emb = vec![1.0, 0.0, 0.0];
        let results = search(&g, &SearchOptions::default(), Some(&query_emb));
        assert!(!results.is_empty());
        // First result should be the OI hypothesis (highest cosine similarity)
        assert!(results[0].statement.contains("OI"));
    }

    #[test]
    fn test_search_excludes_draft() {
        let g = build_test_graph();
        let query_emb = vec![1.0, 0.0, 0.0];
        let results = search(&g, &SearchOptions { top_k: 20, include_decline: true, ..Default::default() }, Some(&query_emb));
        assert!(results.iter().all(|r| r.status != "Draft"));
    }

    #[test]
    fn test_search_excludes_decline_by_default() {
        let g = build_test_graph();
        let query_emb = vec![1.0, 0.0, 0.0];
        let results = search(&g, &SearchOptions::default(), Some(&query_emb));
        assert!(results.iter().all(|r| r.status != "Decline"));
    }

    #[test]
    fn test_search_includes_decline_when_requested() {
        let g = build_test_graph();
        let query_emb = vec![1.0, 0.0, 0.0];
        let results = search(&g, &SearchOptions { include_decline: true, ..Default::default() }, Some(&query_emb));
        let has_decline = results.iter().any(|r| r.status == "Decline");
        assert!(has_decline);
    }

    #[test]
    fn test_balanced_search() {
        let g = build_test_graph();
        let query_emb = vec![1.0, 0.0, 0.0];
        let results = search_balanced(&g, 5, Some(&query_emb));
        // Should have results from both Accept and Decline
        let has_accept = results.iter().any(|r| r.status == "Accept");
        let has_decline = results.iter().any(|r| r.status == "Decline");
        assert!(has_accept);
        assert!(has_decline);
    }
}
