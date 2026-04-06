/// 3-Layer Graph RAG Search
/// Layer 1: Token match + synonym expansion → entry points
/// Layer 2: Graph walk (1-2 hop BFS) → candidate expansion
/// Layer 3: MMR reranking → diverse final results

use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::embedding::cosine_similarity;
use crate::graph::AmureGraph;
use crate::node::tokenize;
use crate::synonym::SynonymDict;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub node_id: Uuid,
    pub kind: String,
    pub statement: String,
    pub keywords: Vec<String>,
    pub score: f64,
    pub hop_distance: usize,
    pub path: Vec<Uuid>,
    pub failed_path: bool,
    pub path_label: Option<String>,
    pub status: String,
}

/// 검색 옵션
pub struct SearchOptions {
    pub top_k: usize,
    pub max_hops: usize,
    pub include_failed: bool,
    pub mmr_lambda: f64,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            top_k: 10,
            max_hops: 2,
            include_failed: true,
            mmr_lambda: 0.7,
        }
    }
}

/// 3-layer graph RAG search
pub fn search(
    graph: &AmureGraph,
    query: &str,
    synonyms: &SynonymDict,
    opts: &SearchOptions,
) -> Vec<SearchResult> {
    let query_tokens = tokenize(query);
    if query_tokens.is_empty() {
        return Vec::new();
    }

    // Layer 1: Token match + synonym expansion
    let expanded = synonyms.expand_all(&query_tokens);
    let entry_points = token_match(graph, &expanded, opts.top_k * 3);

    // Layer 2: Graph walk from entry points
    let candidates = graph_walk(graph, &entry_points, opts.max_hops);

    // Layer 3: MMR reranking
    let mut results = mmr_rerank(graph, candidates, &expanded, opts);

    // Label failed paths
    for r in &mut results {
        if let Some(node) = graph.get_node(&r.node_id) {
            if node.is_failed() {
                r.failed_path = true;
                let reason = node.metadata.get("reject_reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("기각됨");
                r.path_label = Some(format!("이 경로는 이미 실패했다 — {}", reason));
            }
        }
    }

    // Filter failed if requested
    if !opts.include_failed {
        results.retain(|r| !r.failed_path);
    }

    results.truncate(opts.top_k);
    results
}

/// 임베딩 우선 검색. API 장애 시 키워드 fallback.
/// query_embedding이 None이면 기존 keyword-only 검색으로 자동 폴백.
/// query_embedding이 있으면 100% 임베딩 스코어링 + Graph Walk으로 연결 노드 확장.
pub fn search_hybrid(
    graph: &AmureGraph,
    query: &str,
    synonyms: &SynonymDict,
    opts: &SearchOptions,
    query_embedding: Option<&[f32]>,
) -> Vec<SearchResult> {
    // API 장애 → 기존 키워드 검색으로 fallback
    if query_embedding.is_none() {
        return search(graph, query, synonyms, opts);
    }
    let q_emb = query_embedding.unwrap();

    // Step 1: 모든 노드에 대해 임베딩 유사도 계산 → entry points
    let mut emb_scored: Vec<(Uuid, f64)> = graph.nodes.iter()
        .filter_map(|(id, node)| {
            let emb = node.embedding.as_ref()?;
            let sim = cosine_similarity(q_emb, emb);
            if sim > 0.1 { Some((*id, sim)) } else { None }
        })
        .collect();
    emb_scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    emb_scored.truncate(opts.top_k * 3);

    // Step 2: 임베딩 상위 노드에서 Graph Walk → 연결된 노드 확장
    let candidates = graph_walk(graph, &emb_scored, opts.max_hops);

    // Step 3: 모든 후보에 임베딩 스코어 부여 (100% embedding)
    let mut final_candidates: HashMap<Uuid, (f64, usize, Vec<Uuid>)> = HashMap::new();
    for (id, (_walk_score, hop, path)) in &candidates {
        let emb_score = graph.get_node(id)
            .and_then(|n| n.embedding.as_ref())
            .map(|emb| cosine_similarity(q_emb, emb))
            .unwrap_or(0.0);
        final_candidates.insert(*id, (emb_score, *hop, path.clone()));
    }

    // Step 4: MMR reranking → 다양성 확보
    let query_tokens = tokenize(query);
    let expanded = synonyms.expand_all(&query_tokens);
    let mut results = mmr_rerank(graph, final_candidates, &expanded, opts);

    // Label failed paths
    for r in &mut results {
        if let Some(node) = graph.get_node(&r.node_id) {
            if node.is_failed() {
                r.failed_path = true;
                let reason = node.metadata.get("reject_reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("기각됨");
                r.path_label = Some(format!("이 경로는 이미 실패했다 — {}", reason));
            }
        }
    }

    if !opts.include_failed {
        results.retain(|r| !r.failed_path);
    }

    results.truncate(opts.top_k);
    results
}

/// Layer 1: Token matching with synonym-expanded query
fn token_match(
    graph: &AmureGraph,
    expanded_tokens: &[String],
    top_k: usize,
) -> Vec<(Uuid, f64)> {
    let token_set: HashSet<&str> = expanded_tokens.iter().map(|s| s.as_str()).collect();
    let n_query = expanded_tokens.len().max(1) as f64;

    let mut scored: Vec<(Uuid, f64)> = graph.nodes.iter().filter_map(|(id, node)| {
        // Keyword match (weight 0.6)
        let kw_matches = node.keywords.iter()
            .filter(|k| token_set.contains(k.to_lowercase().as_str()))
            .count();

        // Statement token match (weight 0.4)
        let node_tokens = node.tokens();
        let text_matches = node_tokens.iter()
            .filter(|t| token_set.contains(t.as_str()))
            .count();

        let score = (kw_matches as f64 * 0.6 + text_matches as f64 * 0.4) / n_query;
        if score > 0.0 { Some((*id, score)) } else { None }
    }).collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);
    scored
}

/// Layer 2: Graph walk from entry points, collecting candidates with decayed scores
fn graph_walk(
    graph: &AmureGraph,
    entry_points: &[(Uuid, f64)],
    max_hops: usize,
) -> HashMap<Uuid, (f64, usize, Vec<Uuid>)> {
    // candidate_id → (best_score, hop_distance, path)
    let mut candidates: HashMap<Uuid, (f64, usize, Vec<Uuid>)> = HashMap::new();

    for (entry_id, entry_score) in entry_points {
        let walked = graph.walk(entry_id, max_hops, None);
        for (node_id, hop) in walked {
            let decayed_score = entry_score * 0.5f64.powi(hop as i32);
            let path = vec![*entry_id, node_id]; // simplified path

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

/// Layer 3: MMR (Maximal Marginal Relevance) reranking
fn mmr_rerank(
    graph: &AmureGraph,
    candidates: HashMap<Uuid, (f64, usize, Vec<Uuid>)>,
    _query_tokens: &[String],
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
    let mut selected_kw_sets: Vec<HashSet<String>> = Vec::new();

    while !remaining.is_empty() && selected.len() < opts.top_k {
        let mut best_idx = 0;
        let mut best_mmr = f64::NEG_INFINITY;

        for (i, (id, score, _, _)) in remaining.iter().enumerate() {
            let relevance = *score;

            // Max similarity to already selected (Jaccard on keywords)
            let max_sim = if selected_kw_sets.is_empty() {
                0.0
            } else {
                let node_kws: HashSet<String> = graph.get_node(id)
                    .map(|n| n.keywords.iter().map(|k| k.to_lowercase()).collect())
                    .unwrap_or_default();
                selected_kw_sets.iter()
                    .map(|sel_kws| jaccard(&node_kws, sel_kws))
                    .fold(0.0f64, f64::max)
            };

            let mmr = lambda * relevance - (1.0 - lambda) * max_sim;
            if mmr > best_mmr {
                best_mmr = mmr;
                best_idx = i;
            }
        }

        let (id, score, hop, path) = remaining.remove(best_idx);
        if let Some(node) = graph.get_node(&id) {
            let kw_set: HashSet<String> = node.keywords.iter().map(|k| k.to_lowercase()).collect();
            selected_kw_sets.push(kw_set);

            selected.push(SearchResult {
                node_id: id,
                kind: format!("{:?}", node.kind),
                statement: node.statement.clone(),
                keywords: node.keywords.clone(),
                score,
                hop_distance: hop,
                path,
                failed_path: false,
                path_label: None,
                status: format!("{:?}", node.status),
            });
        }
    }

    selected
}

/// Jaccard similarity between two keyword sets
fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    if union > 0.0 { intersection / union } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edge::{Edge, EdgeKind};
    use crate::node::{Node, NodeKind, NodeStatus};

    fn build_test_graph() -> AmureGraph {
        let mut g = AmureGraph::new();

        let c1 = g.add_node(Node::new(
            NodeKind::Claim,
            "OI 변화량은 momentum의 선행지표다".into(),
            vec!["OI".into(), "momentum".into(), "open_interest".into()],
        ));

        let r1 = g.add_node(Node::new(
            NodeKind::Reason,
            "OI 증가는 conviction 유입을 의미한다".into(),
            vec!["OI".into(), "conviction".into()],
        ));

        let r2 = g.add_node(
            Node::new(
                NodeKind::Reason,
                "거래소별 OI 집계가 달라서 노이즈가 크다".into(),
                vec!["noise".into(), "exchange".into()],
            ).with_status(NodeStatus::Weakened)
        );

        let c2 = g.add_node(Node::new(
            NodeKind::Claim,
            "funding rate 극단값은 mean reversion 시그널이다".into(),
            vec!["funding".into(), "mean_reversion".into()],
        ));

        g.add_edge(Edge::new(r1, c1, EdgeKind::Support));
        g.add_edge(Edge::new(r2, c1, EdgeKind::Rebut));

        g
    }

    #[test]
    fn test_search_basic() {
        let g = build_test_graph();
        let syn = SynonymDict::new();
        let results = search(&g, "OI momentum", &syn, &SearchOptions::default());

        assert!(!results.is_empty());
        // First result should be the OI claim (direct keyword match)
        assert!(results[0].statement.contains("OI"));
    }

    #[test]
    fn test_search_synonym_expansion() {
        let g = build_test_graph();
        let syn = SynonymDict::new();
        // "미결제약정" should find OI nodes via synonym
        let results = search(&g, "미결제약정 추세", &syn, &SearchOptions::default());
        assert!(!results.is_empty());
    }

    #[test]
    fn test_search_graph_walk() {
        let g = build_test_graph();
        let syn = SynonymDict::new();
        // Search for "conviction" should find the Reason AND the connected Claim via walk
        let results = search(&g, "conviction", &syn, &SearchOptions { max_hops: 2, ..Default::default() });
        let ids: Vec<_> = results.iter().map(|r| r.node_id).collect();
        // Should have at least 2 results (reason + claim via graph walk)
        assert!(results.len() >= 2);
    }

    #[test]
    fn test_search_failed_paths() {
        let g = build_test_graph();
        let syn = SynonymDict::new();
        let results = search(&g, "noise exchange", &syn, &SearchOptions {
            include_failed: true,
            ..Default::default()
        });
        let failed = results.iter().filter(|r| r.failed_path).count();
        assert!(failed > 0, "Should find weakened node");
    }

    #[test]
    fn test_search_exclude_failed() {
        let g = build_test_graph();
        let syn = SynonymDict::new();
        let results = search(&g, "noise exchange", &syn, &SearchOptions {
            include_failed: false,
            ..Default::default()
        });
        let failed = results.iter().filter(|r| r.failed_path).count();
        assert_eq!(failed, 0);
    }

    #[test]
    fn test_mmr_diversity() {
        let mut g = AmureGraph::new();
        // Add 5 nodes with same keywords — MMR should diversify
        for i in 0..5 {
            g.add_node(Node::new(
                NodeKind::Claim,
                format!("OI claim variant {}", i),
                vec!["OI".into(), "momentum".into()],
            ));
        }
        // Add 1 node with different keywords
        g.add_node(Node::new(
            NodeKind::Claim,
            "funding rate claim".into(),
            vec!["funding".into()],
        ));

        let syn = SynonymDict::new();
        let results = search(&g, "OI momentum funding", &syn, &SearchOptions {
            top_k: 3,
            ..Default::default()
        });
        // With MMR, funding claim should appear even though OI claims score higher individually
        let has_funding = results.iter().any(|r| r.statement.contains("funding"));
        assert!(has_funding, "MMR should surface diverse funding result");
    }
}
