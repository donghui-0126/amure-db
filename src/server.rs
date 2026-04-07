/// amure-db standalone server
/// Port 8081 — graph API + dashboard
/// Single owner of graph data. AlphaFactor calls via HTTP.

use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use axum::routing::{get, post, patch, delete};
use axum::extract::{State, Path, Query};
use axum::response::Html;
use axum::Json;
use tower_http::cors::CorsLayer;
use serde::Deserialize;
use uuid::Uuid;

use amure_db::graph::AmureGraph;
use amure_db::node::{Node, NodeKind, NodeStatus, tokenize};
use amure_db::edge::{Edge, EdgeKind};
use amure_db::search::{search, search_hybrid, SearchOptions};
use amure_db::synonym::SynonymDict;
use amure_db::embedding;

const DATA_DIR: &str = "data/amure_graph";
const DASHBOARD: &str = include_str!("../dashboard.html");

#[derive(Clone)]
struct AppState {
    graph: Arc<RwLock<AmureGraph>>,
    synonyms: Arc<SynonymDict>,
}

#[tokio::main]
async fn main() {
    // .env 파일에서 환경변수 로드
    if let Ok(content) = std::fs::read_to_string(".env") {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            if let Some((key, value)) = line.split_once('=') {
                std::env::set_var(key.trim(), value.trim());
            }
        }
    }

    let graph = if std::path::Path::new(DATA_DIR).join("nodes.json").exists() {
        AmureGraph::load(std::path::Path::new(DATA_DIR)).unwrap_or_default()
    } else {
        AmureGraph::new()
    };
    println!("amure-db loaded: {} nodes, {} edges", graph.node_count(), graph.edge_count());

    let state = AppState {
        graph: Arc::new(RwLock::new(graph)),
        synonyms: Arc::new(SynonymDict::new()),
    };

    let app = axum::Router::new()
        // Dashboard
        .route("/", get(dashboard))
        // Core graph CRUD
        .route("/api/graph/all", get(graph_all))
        .route("/api/graph/summary", get(graph_summary))
        .route("/api/graph/search", get(graph_search))
        .route("/api/graph/node/{id}", get(graph_node))
        .route("/api/graph/node/{id}", delete(delete_node))
        .route("/api/graph/node", post(create_node))
        .route("/api/graph/node/{id}", patch(update_node))
        .route("/api/graph/edge", post(create_edge))
        .route("/api/graph/edge/{id}", delete(delete_edge))
        .route("/api/graph/walk/{id}", get(graph_walk))
        .route("/api/graph/subgraph/{id}", get(graph_subgraph))
        // Knowledge analysis endpoints (ported from graph_adapter)
        .route("/api/check-failures", post(check_failures))
        .route("/api/check-revalidation", get(check_revalidation))
        .route("/api/detect-contradictions", post(detect_contradictions))
        .route("/api/auto-gap-claims", post(auto_gap_claims))
        .route("/api/suggest-combinations", get(suggest_combinations))
        // Legacy endpoints (keep existing)
        .route("/api/claim", post(create_claim))
        .route("/api/edge", post(create_edge_legacy))
        .route("/api/save", post(save_graph))
        // LLM endpoints
        .route("/api/llm/auto-tag", post(llm_auto_tag))
        .route("/api/llm/summarize", post(llm_summarize_search))
        .route("/api/llm/verify-claim", post(llm_verify_claim))
        // Causal chain + temporal tracking
        .route("/api/graph/causal-chains/{id}", get(causal_chains))
        .route("/api/graph/temporal-health", get(temporal_health))
        .route("/api/graph/impact/{id}", get(impact_analysis))
        .route("/api/graph/dependencies/{id}", get(dependency_tree))
        // Edge propagation
        .route("/api/graph/propagate-verdict/{id}", post(propagate_verdict))
        .route("/api/graph/detect-dependencies/{id}", post(detect_claim_dependencies))
        .route("/api/graph/on-accept/{id}", post(on_accept))
        // Embedding endpoints
        .route("/api/graph/similar/{id}", get(similar_nodes))
        .route("/api/graph/unrelated/{id}", get(unrelated_nodes))
        .route("/api/graph/embed-all", post(embed_all_nodes))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = "0.0.0.0:8081";
    println!("amure-db server: http://localhost:8081");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ── Dashboard ───────────────────────────────────────────────────────────────

async fn dashboard() -> Html<&'static str> {
    Html(DASHBOARD)
}

// ── Graph API — Core CRUD ──────────────────────────────────────────────────

async fn graph_all(State(s): State<AppState>) -> Json<serde_json::Value> {
    let g = s.graph.read().await;
    let nodes: Vec<serde_json::Value> = g.nodes.values().map(node_json).collect();
    let edges: Vec<serde_json::Value> = g.edges.values().map(edge_json).collect();
    Json(serde_json::json!({"nodes": nodes, "edges": edges, "n_nodes": nodes.len(), "n_edges": edges.len()}))
}

async fn graph_summary(State(s): State<AppState>) -> Json<serde_json::Value> {
    let g = s.graph.read().await;
    Json(serde_json::json!(g.summary()))
}

#[derive(Deserialize)]
struct SearchQuery { q: Option<String>, top_k: Option<usize>, include_failed: Option<bool>, kind: Option<String> }

async fn graph_search(State(s): State<AppState>, Query(q): Query<SearchQuery>) -> Json<serde_json::Value> {
    let query = q.q.unwrap_or_default();
    if query.is_empty() {
        let g = s.graph.read().await;
        let mut nodes: Vec<serde_json::Value> = g.nodes.values()
            .filter(|n| q.kind.as_deref().map_or(true, |k| format!("{:?}", n.kind).eq_ignore_ascii_case(k)))
            .map(node_json)
            .collect();
        nodes.sort_by(|a, b| a["kind"].as_str().cmp(&b["kind"].as_str()));
        return Json(serde_json::json!({"results": nodes, "count": nodes.len()}));
    }

    // 쿼리 임베딩 시도 (실패 시 keyword-only fallback)
    let query_embedding = embedding::get_embedding(&query).await.ok();

    let g = s.graph.read().await;
    let opts = SearchOptions {
        top_k: q.top_k.unwrap_or(10),
        include_failed: q.include_failed.unwrap_or(true),
        kind_filter: q.kind,
        ..Default::default()
    };
    let results = search_hybrid(&g, &query, &s.synonyms, &opts, query_embedding.as_deref());
    Json(serde_json::json!({"results": results, "count": results.len()}))
}

async fn graph_node(State(s): State<AppState>, Path(id): Path<Uuid>) -> Json<serde_json::Value> {
    let g = s.graph.read().await;
    match g.get_node(&id) {
        Some(n) => {
            let edges: Vec<serde_json::Value> = g.edges.values()
                .filter(|e| e.source == id || e.target == id)
                .map(|e| {
                    let other_id = if e.source == id { e.target } else { e.source };
                    let other = g.get_node(&other_id);
                    let mut ej = edge_json(e);
                    if let Some(o) = other {
                        ej.as_object_mut().unwrap().insert("other_statement".into(), serde_json::Value::String(o.statement.clone()));
                        ej.as_object_mut().unwrap().insert("other_kind".into(), serde_json::Value::String(format!("{:?}", o.kind)));
                    }
                    ej
                }).collect();
            Json(serde_json::json!({"node": node_json(n), "edges": edges}))
        }
        None => Json(serde_json::json!({"error": "Not found"})),
    }
}

async fn delete_node(State(s): State<AppState>, Path(id): Path<Uuid>) -> Json<serde_json::Value> {
    let mut g = s.graph.write().await;
    if g.remove_node(&id).is_some() {
        let _ = g.save(std::path::Path::new(DATA_DIR));
        Json(serde_json::json!({"status": "deleted"}))
    } else {
        Json(serde_json::json!({"error": "Not found"}))
    }
}

// ── POST /api/graph/node — add any node ────────────────────────────────────

#[derive(Deserialize)]
struct CreateNodeReq {
    kind: String,            // "Claim", "Reason", "Evidence", "Experiment"
    statement: String,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    metadata: serde_json::Value,
    #[serde(default)]
    status: Option<String>,  // "Draft", "Active", "Accepted", "Rejected", "Weakened"
}

async fn create_node(State(s): State<AppState>, Json(req): Json<CreateNodeReq>) -> Json<serde_json::Value> {
    let kind = parse_node_kind(&req.kind);
    let status = req.status.as_deref().map(parse_node_status).unwrap_or(NodeStatus::Draft);
    let mut node = Node::new(kind, req.statement, req.keywords)
        .with_status(status);
    if !req.metadata.is_null() {
        node = node.with_metadata(req.metadata);
    }
    let embed_text = node.embed_text();
    let mut g = s.graph.write().await;
    let id = g.add_node(node);
    let _ = g.save(std::path::Path::new(DATA_DIR));
    drop(g);

    // 임베딩 비동기 계산 (응답 블로킹 없음)
    let graph = s.graph.clone();
    tokio::spawn(async move {
        if let Ok(emb) = embedding::get_embedding(&embed_text).await {
            let mut g = graph.write().await;
            if let Some(node) = g.get_node_mut(&id) {
                node.embedding = Some(emb);
                node.updated_at = chrono::Utc::now();
            }
            let _ = g.save(std::path::Path::new(DATA_DIR));
        }
    });

    Json(serde_json::json!({"status": "created", "id": id}))
}

// ── PATCH /api/graph/node/{id} — update node status/metadata ───────────────

#[derive(Deserialize)]
struct UpdateNodeReq {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
    #[serde(default)]
    keywords: Option<Vec<String>>,
    #[serde(default)]
    statement: Option<String>,
}

async fn update_node(State(s): State<AppState>, Path(id): Path<Uuid>, Json(req): Json<UpdateNodeReq>) -> Json<serde_json::Value> {
    let mut g = s.graph.write().await;
    match g.get_node_mut(&id) {
        Some(node) => {
            if let Some(ref status) = req.status {
                node.status = parse_node_status(status);
            }
            if let Some(ref meta) = req.metadata {
                // Merge metadata
                if let (Some(existing), Some(incoming)) = (node.metadata.as_object_mut(), meta.as_object()) {
                    for (k, v) in incoming {
                        existing.insert(k.clone(), v.clone());
                    }
                } else {
                    node.metadata = meta.clone();
                }
            }
            if let Some(ref kws) = req.keywords {
                node.keywords = kws.clone();
            }
            if let Some(ref stmt) = req.statement {
                node.statement = stmt.clone();
            }
            node.updated_at = chrono::Utc::now();
            let _ = g.save(std::path::Path::new(DATA_DIR));
            Json(serde_json::json!({"status": "updated", "id": id}))
        }
        None => Json(serde_json::json!({"error": "Not found"})),
    }
}

// ── POST /api/graph/edge — add edge ────────────────────────────────────────

#[derive(Deserialize)]
struct CreateEdgeReq {
    source: Uuid,
    target: Uuid,
    kind: String,
    #[serde(default)]
    note: Option<String>,
}

async fn create_edge(State(s): State<AppState>, Json(req): Json<CreateEdgeReq>) -> Json<serde_json::Value> {
    let kind = parse_edge_kind(&req.kind);
    let mut g = s.graph.write().await;
    let edge = Edge::new(req.source, req.target, kind).with_note(req.note.unwrap_or_default());
    let id = g.add_edge(edge);
    let _ = g.save(std::path::Path::new(DATA_DIR));
    Json(serde_json::json!({"status": "created", "id": id}))
}

// ── DELETE /api/graph/edge/{id} — remove edge ──────────────────────────────

async fn delete_edge(State(s): State<AppState>, Path(id): Path<Uuid>) -> Json<serde_json::Value> {
    let mut g = s.graph.write().await;
    if g.remove_edge(&id).is_some() {
        let _ = g.save(std::path::Path::new(DATA_DIR));
        Json(serde_json::json!({"status": "deleted"}))
    } else {
        Json(serde_json::json!({"error": "Not found"}))
    }
}

// ── GET /api/graph/walk/{id}?hops=2 — BFS walk ────────────────────────────

#[derive(Deserialize, Default)]
struct WalkQuery { hops: Option<usize> }

async fn graph_walk(State(s): State<AppState>, Path(id): Path<Uuid>, Query(q): Query<WalkQuery>) -> Json<serde_json::Value> {
    let max_hops = q.hops.unwrap_or(2);
    let g = s.graph.read().await;
    if g.get_node(&id).is_none() {
        return Json(serde_json::json!({"error": "Node not found"}));
    }
    let walked = g.walk(&id, max_hops, None);
    let nodes: Vec<serde_json::Value> = walked.iter().filter_map(|(nid, depth)| {
        g.get_node(nid).map(|n| serde_json::json!({
            "id": n.id,
            "kind": format!("{:?}", n.kind),
            "statement": n.statement,
            "status": format!("{:?}", n.status),
            "depth": depth,
        }))
    }).collect();
    Json(serde_json::json!({"start": id, "max_hops": max_hops, "nodes": nodes, "count": nodes.len()}))
}

// ── GET /api/graph/subgraph/{id} — full subgraph ──────────────────────────

async fn graph_subgraph(State(s): State<AppState>, Path(id): Path<Uuid>) -> Json<serde_json::Value> {
    let g = s.graph.read().await;
    if g.get_node(&id).is_none() {
        return Json(serde_json::json!({"error": "Node not found"}));
    }
    let walked = g.walk(&id, 10, None);
    let node_ids: Vec<Uuid> = walked.iter().map(|(nid, _)| *nid).collect();
    let (nodes, edges) = g.subgraph(&node_ids);

    let node_list: Vec<serde_json::Value> = nodes.iter().map(|n| {
        serde_json::json!({
            "id": n.id,
            "kind": format!("{:?}", n.kind),
            "statement": n.statement,
            "keywords": n.keywords,
            "status": format!("{:?}", n.status),
            "metadata": n.metadata,
            "failed": n.is_failed(),
        })
    }).collect();

    let edge_list: Vec<serde_json::Value> = edges.iter().map(|e| {
        serde_json::json!({
            "id": e.id,
            "source": e.source,
            "target": e.target,
            "kind": format!("{:?}", e.kind),
            "weight": e.weight,
            "note": e.note,
        })
    }).collect();

    Json(serde_json::json!({
        "root": id,
        "nodes": node_list,
        "edges": edge_list,
        "n_nodes": node_list.len(),
        "n_edges": edge_list.len(),
    }))
}

// ══════════════════════════════════════════════════════════════════════════════
// Knowledge Analysis Endpoints (ported from engine/knowledge/graph_adapter.rs)
// ══════════════════════════════════════════════════════════════════════════════

// ── POST /api/check-failures — failure pattern warning ─────────────────────

#[derive(Deserialize)]
struct FailureCheckReq {
    statement: String,
    #[serde(default)]
    keywords: Vec<String>,
}

#[derive(serde::Serialize)]
struct FailureWarning {
    failed_node_id: Uuid,
    failed_statement: String,
    status: String,
    overlap_keywords: Vec<String>,
    score: f64,
    failure_reason: String,
    experiments_done: Vec<String>,
    gaps_remaining: Vec<String>,
    methods_used: Vec<String>,
    methods_not_used: Vec<String>,
}

async fn check_failures(State(s): State<AppState>, Json(req): Json<FailureCheckReq>) -> Json<serde_json::Value> {
    let g = s.graph.read().await;
    let warnings = do_check_failure_patterns(&g, &req.statement, &req.keywords);
    Json(serde_json::json!({"warnings": warnings, "count": warnings.len()}))
}

fn do_check_failure_patterns(g: &AmureGraph, statement: &str, keywords: &[String]) -> Vec<FailureWarning> {
    let mut warnings = Vec::new();

    let failed_nodes: Vec<&Node> = g.nodes.values()
        .filter(|n| n.is_failed())
        .collect();

    if failed_nodes.is_empty() { return warnings; }

    let new_kws: HashSet<String> = keywords.iter()
        .map(|k| k.to_lowercase()).collect();
    let new_tokens: HashSet<String> = tokenize(statement).into_iter().collect();

    for node in &failed_nodes {
        let node_kws: HashSet<String> = node.keywords.iter()
            .map(|k| k.to_lowercase()).collect();
        let node_tokens: HashSet<String> = node.tokens().into_iter().collect();

        let kw_overlap: Vec<String> = new_kws.intersection(&node_kws).cloned().collect();
        let token_overlap = new_tokens.intersection(&node_tokens).count();

        let score = kw_overlap.len() as f64 * 0.6 + token_overlap as f64 * 0.1;
        if score > 0.5 {
            let reason = node.metadata.get("reject_reason")
                .or(node.metadata.get("accept_reason"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let mut experiments_done = Vec::new();
            let mut gaps_remaining = Vec::new();
            let mut methods_used = HashSet::new();

            let walked = g.walk(&node.id, 3, None);
            for (nid, _hop) in &walked {
                if let Some(n) = g.get_node(nid) {
                    if n.kind == NodeKind::Experiment {
                        experiments_done.push(n.statement.clone());
                        if let Some(m) = n.metadata.get("method").and_then(|v| v.as_str()) {
                            methods_used.insert(m.to_string());
                        }
                        if let Some(gaps) = n.metadata.get("gaps") {
                            if let Some(arr) = gaps.as_array() {
                                for gap in arr {
                                    if let Some(s) = gap.as_str() {
                                        gaps_remaining.push(s.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let all_methods = ["CrossSectional", "Distributional", "Conditional", "DoseResponse",
                "Regime", "Temporal", "MultiHorizon", "EntryExit", "Backtest"];
            let methods_not_used: Vec<String> = all_methods.iter()
                .filter(|m| !methods_used.contains(**m))
                .map(|m| m.to_string())
                .collect();

            warnings.push(FailureWarning {
                failed_node_id: node.id,
                failed_statement: node.statement.clone(),
                status: format!("{:?}", node.status),
                overlap_keywords: kw_overlap,
                score,
                failure_reason: reason,
                experiments_done,
                gaps_remaining,
                methods_used: methods_used.into_iter().collect(),
                methods_not_used,
            });
        }
    }

    warnings.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    warnings.truncate(5);
    warnings
}

// ── GET /api/check-revalidation — alpha decay check ────────────────────────

#[derive(serde::Serialize)]
struct RevalidationAlert {
    node_id: Uuid,
    statement: String,
    days_since_update: i64,
    trigger: String,
    reason: String,
}

async fn check_revalidation(State(s): State<AppState>) -> Json<serde_json::Value> {
    let g = s.graph.read().await;
    let alerts = do_check_revalidation(&g);
    Json(serde_json::json!({"alerts": alerts, "count": alerts.len()}))
}

fn do_check_revalidation(g: &AmureGraph) -> Vec<RevalidationAlert> {
    let now = chrono::Utc::now();
    let mut alerts = Vec::new();

    for node in g.nodes_by_kind(NodeKind::Claim) {
        if node.status != NodeStatus::Accepted { continue; }

        let days_since = (now - node.updated_at).num_days();
        let needs_revalidation = days_since > 30;
        let has_decay_risk = node.statement.contains("decay")
            || node.metadata.get("alpha_decay").and_then(|v| v.as_bool()).unwrap_or(false);

        let trigger = node.metadata.get("trigger")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if needs_revalidation || has_decay_risk {
            alerts.push(RevalidationAlert {
                node_id: node.id,
                statement: node.statement.clone(),
                days_since_update: days_since,
                trigger: trigger.to_string(),
                reason: if has_decay_risk {
                    "Alpha decay 위험 — 주기적 재검증 필요".into()
                } else {
                    format!("{}일간 재검증 안 됨", days_since)
                },
            });
        }
    }

    alerts.sort_by(|a, b| b.days_since_update.cmp(&a.days_since_update));
    alerts
}

// ── POST /api/detect-contradictions — contradiction detection ──────────────

#[derive(serde::Serialize)]
struct ContradictionAlert {
    node_a_id: Uuid,
    node_a_statement: String,
    node_b_id: Uuid,
    node_b_statement: String,
    overlap_keywords: Vec<String>,
    reason: String,
}

async fn detect_contradictions(State(s): State<AppState>) -> Json<serde_json::Value> {
    let mut g = s.graph.write().await;
    let alerts = do_detect_contradictions(&mut g);
    let _ = g.save(std::path::Path::new(DATA_DIR));
    Json(serde_json::json!({"contradictions": alerts, "count": alerts.len()}))
}

fn do_detect_contradictions(g: &mut AmureGraph) -> Vec<ContradictionAlert> {
    // Scope-based contradiction detection.
    // Uses metadata.scope.signal + metadata.direction instead of text matching.

    struct ClaimInfo {
        id: Uuid,
        statement: String,
        direction: String,
        signals: Vec<String>,
        regime: String,
    }

    let accepted: Vec<ClaimInfo> = g
        .nodes_by_kind(NodeKind::Claim)
        .iter()
        .filter(|n| n.status == NodeStatus::Accepted)
        .map(|n| {
            let direction = n.metadata.get("direction")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    // Fallback: infer from statement
                    let s = n.statement.to_lowercase();
                    if s.contains("reversal") || s.contains("반전") || s.contains("mean reversion") { "reversal" }
                    else if s.contains("momentum") || s.contains("continuation") { "momentum" }
                    else if s.contains("prediction") || s.contains("예측") { "prediction" }
                    else { "neutral" }
                }).to_string();

            let signals: Vec<String> = n.metadata.get("scope")
                .and_then(|s| s.get("signal"))
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_lowercase())).collect())
                .unwrap_or_default();

            let regime = n.metadata.get("scope")
                .and_then(|s| s.get("regime"))
                .and_then(|v| v.as_str())
                .unwrap_or("all").to_string();

            ClaimInfo { id: n.id, statement: n.statement.clone(), direction, signals, regime }
        })
        .collect();

    let mut alerts = Vec::new();

    for i in 0..accepted.len() {
        for j in (i+1)..accepted.len() {
            let a = &accepted[i];
            let b = &accepted[j];

            // Skip if either has no signals (can't determine scope)
            if a.signals.is_empty() || b.signals.is_empty() { continue; }

            // Check signal overlap
            let sig_a: HashSet<&str> = a.signals.iter().map(|s| s.as_str()).collect();
            let sig_b: HashSet<&str> = b.signals.iter().map(|s| s.as_str()).collect();
            let signal_overlap: Vec<String> = sig_a.intersection(&sig_b).map(|s| s.to_string()).collect();

            if signal_overlap.is_empty() { continue; }

            // Check regime overlap (if both specify regime and they don't overlap → not contradicting)
            let regime_overlaps = a.regime == "all" || b.regime == "all" || a.regime == b.regime;
            if !regime_overlaps { continue; }

            // Check direction conflict
            let dir_conflicts = a.direction != b.direction
                && a.direction != "neutral" && b.direction != "neutral"
                && a.direction != "inconclusive" && b.direction != "inconclusive";

            if !dir_conflicts { continue; }

            // This is a real contradiction
            let has_edge = g.edges.values().any(|e| {
                e.kind == EdgeKind::Contradicts &&
                ((e.source == a.id && e.target == b.id) || (e.source == b.id && e.target == a.id))
            });

            if !has_edge {
                g.add_edge(
                    Edge::new(a.id, b.id, EdgeKind::Contradicts)
                        .with_note(format!("scope 기반: signal({}) 겹침 + direction 충돌 ({}↔{})",
                            signal_overlap.join(","), a.direction, b.direction))
                );
            }

            alerts.push(ContradictionAlert {
                node_a_id: a.id,
                node_a_statement: a.statement.clone(),
                node_b_id: b.id,
                node_b_statement: b.statement.clone(),
                overlap_keywords: signal_overlap,
                reason: format!("scope 기반: {}↔{} direction 충돌", a.direction, b.direction),
            });
        }
    }

    alerts
}

// ── POST /api/auto-gap-claims — auto gap claim creation ────────────────────

#[derive(Deserialize)]
struct AutoGapReq {
    source_claim_id: Uuid,
    gaps: Vec<String>,
    #[serde(default)]
    keywords: Vec<String>,
}

async fn auto_gap_claims(State(s): State<AppState>, Json(req): Json<AutoGapReq>) -> Json<serde_json::Value> {
    let mut g = s.graph.write().await;
    let mut created = Vec::new();

    for gap in &req.gaps {
        if gap.len() < 5 { continue; }
        let mut kw = req.keywords.clone();
        kw.push("gap_derived".into());
        let node = Node::new(NodeKind::Claim, gap.clone(), kw)
            .with_metadata(serde_json::json!({
                "trigger": "원본 claim의 gap에서 파생",
                "source_claim": req.source_claim_id.to_string(),
                "auto_generated": true,
            }));
        let gap_id = g.add_node(node);
        g.add_edge(
            Edge::new(gap_id, req.source_claim_id, EdgeKind::Refines)
                .with_note("gap에서 파생된 하위 가설".to_string())
        );
        created.push(gap_id);
    }

    let _ = g.save(std::path::Path::new(DATA_DIR));
    Json(serde_json::json!({"created": created, "count": created.len()}))
}

// ── GET /api/suggest-combinations — failure combination suggestions ────────

#[derive(serde::Serialize)]
struct CombinationSuggestion {
    failed_nodes: Vec<(Uuid, String)>,
    shared_keywords: Vec<String>,
    individual_irs: Vec<String>,
    combination_idea: String,
    untried_combination: bool,
}

async fn suggest_combinations(State(s): State<AppState>) -> Json<serde_json::Value> {
    let g = s.graph.read().await;
    let suggestions = do_suggest_failure_combinations(&g);
    Json(serde_json::json!({"suggestions": suggestions, "count": suggestions.len()}))
}

fn do_suggest_failure_combinations(g: &AmureGraph) -> Vec<CombinationSuggestion> {
    let meta_kw = ["validated", "disproven", "gap_derived", "auto_generated"];

    let failed: Vec<(&Node, Vec<String>)> = g.nodes.values()
        .filter(|n| n.kind == NodeKind::Claim && (n.status == NodeStatus::Rejected || n.status == NodeStatus::Weakened))
        .map(|n| {
            let walked = g.walk(&n.id, 3, None);
            let mut methods = Vec::new();
            for (nid, _) in &walked {
                if let Some(exp) = g.get_node(nid) {
                    if exp.kind == NodeKind::Experiment {
                        let m = exp.metadata.get("method").and_then(|v| v.as_str()).unwrap_or("?");
                        let ir = exp.metadata.get("result")
                            .and_then(|r| r.get("ir"))
                            .and_then(|v| v.as_f64())
                            .map(|v| format!("IR={:.3}", v))
                            .unwrap_or_default();
                        methods.push(format!("{}({})", m, ir));
                    }
                }
            }
            (n, methods)
        })
        .collect();

    let mut suggestions = Vec::new();

    for i in 0..failed.len() {
        for j in (i+1)..failed.len() {
            let (a, a_methods) = &failed[i];
            let (b, b_methods) = &failed[j];

            let kw_a: HashSet<String> = a.keywords.iter()
                .filter(|k| !meta_kw.contains(&k.as_str()))
                .map(|k| k.to_lowercase()).collect();
            let kw_b: HashSet<String> = b.keywords.iter()
                .filter(|k| !meta_kw.contains(&k.as_str()))
                .map(|k| k.to_lowercase()).collect();

            let shared: Vec<String> = kw_a.intersection(&kw_b).cloned().collect();
            if shared.is_empty() { continue; }

            let unique_a: Vec<String> = kw_a.difference(&kw_b).cloned().collect();
            let unique_b: Vec<String> = kw_b.difference(&kw_a).cloned().collect();

            let idea = format!(
                "공통({}) 기반에서 [{}]과 [{}]을 결합. 각각 단독으로는 미유의였지만 동시 발생 조건에서 시그널 증폭 가능",
                shared.join(","),
                unique_a.join(","),
                unique_b.join(","),
            );

            let combined_kw: HashSet<String> = kw_a.union(&kw_b).cloned().collect();
            let already_tried = g.nodes.values().any(|n| {
                if n.kind != NodeKind::Claim { return false; }
                let nkw: HashSet<String> = n.keywords.iter().map(|k| k.to_lowercase()).collect();
                let overlap = combined_kw.intersection(&nkw).count();
                overlap >= combined_kw.len().saturating_sub(1) && n.status != NodeStatus::Rejected
            });

            suggestions.push(CombinationSuggestion {
                failed_nodes: vec![
                    (a.id, a.statement.chars().take(60).collect()),
                    (b.id, b.statement.chars().take(60).collect()),
                ],
                shared_keywords: shared,
                individual_irs: [a_methods.clone(), b_methods.clone()].concat(),
                combination_idea: idea,
                untried_combination: !already_tried,
            });
        }
    }

    suggestions.sort_by(|a, b| {
        b.shared_keywords.len().cmp(&a.shared_keywords.len())
            .then(b.untried_combination.cmp(&a.untried_combination))
    });
    suggestions.truncate(10);
    suggestions
}

// ── Legacy Claim/Edge creation (keep existing dashboard working) ───────────

#[derive(Deserialize)]
struct CreateClaim { statement: String, keywords: Vec<String>, trigger: Option<String> }

async fn create_claim(State(s): State<AppState>, Json(req): Json<CreateClaim>) -> Json<serde_json::Value> {
    let mut g = s.graph.write().await;
    let node = Node::new(NodeKind::Claim, req.statement, req.keywords)
        .with_metadata(serde_json::json!({"trigger": req.trigger.unwrap_or_default()}));
    let id = g.add_node(node);
    let _ = g.save(std::path::Path::new(DATA_DIR));
    Json(serde_json::json!({"status": "created", "id": id}))
}

#[derive(Deserialize)]
struct CreateEdgeLegacy { source: Uuid, target: Uuid, kind: String, note: Option<String> }

async fn create_edge_legacy(State(s): State<AppState>, Json(req): Json<CreateEdgeLegacy>) -> Json<serde_json::Value> {
    let kind = parse_edge_kind(&req.kind);
    let mut g = s.graph.write().await;
    let edge = Edge::new(req.source, req.target, kind).with_note(req.note.unwrap_or_default());
    let id = g.add_edge(edge);
    let _ = g.save(std::path::Path::new(DATA_DIR));
    Json(serde_json::json!({"status": "created", "id": id}))
}

async fn save_graph(State(s): State<AppState>) -> Json<serde_json::Value> {
    let g = s.graph.read().await;
    match g.save(std::path::Path::new(DATA_DIR)) {
        Ok(_) => Json(serde_json::json!({"status": "saved"})),
        Err(e) => Json(serde_json::json!({"error": e.to_string()})),
    }
}

// ── LLM ─────────────────────────────────────────────────────────────────────

async fn call_llm(prompt: &str) -> Result<String, String> {
    let output = tokio::process::Command::new("claude")
        .args(["-p", prompt])
        .output()
        .await;
    match output {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).to_string()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
        Err(e) => Err(format!("claude CLI not found: {}", e)),
    }
}

#[derive(Deserialize)]
struct AutoTagReq { node_id: Uuid }

async fn llm_auto_tag(State(s): State<AppState>, Json(req): Json<AutoTagReq>) -> Json<serde_json::Value> {
    let stmt = {
        let g = s.graph.read().await;
        match g.get_node(&req.node_id) {
            Some(n) => n.statement.clone(),
            None => return Json(serde_json::json!({"error": "Node not found"})),
        }
    };

    let prompt = format!(
        "다음 금융 데이터의 핵심 키워드를 추출해줘. 한국어+영어 혼합, 쉼표로 구분, 10개 이내.\n\
        섹터, 산업, 투자 특성, 테마를 포함해.\n\
        예: tech, ai, 반도체, 대형주, 성장주, gpu\n\n\
        데이터: {}\n\n키워드만 답해:",
        stmt
    );

    match call_llm(&prompt).await {
        Ok(resp) => {
            let keywords: Vec<String> = resp.split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| s.len() >= 2 && s.len() < 30)
                .collect();

            let mut g = s.graph.write().await;
            if let Some(node) = g.get_node_mut(&req.node_id) {
                for kw in &keywords {
                    if !node.keywords.contains(kw) {
                        node.keywords.push(kw.clone());
                    }
                }
                node.updated_at = chrono::Utc::now();
            }
            let _ = g.save(std::path::Path::new(DATA_DIR));

            Json(serde_json::json!({"status": "tagged", "new_keywords": keywords}))
        }
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

#[derive(Deserialize)]
struct SummarizeReq { query: String, top_k: Option<usize> }

async fn llm_summarize_search(State(s): State<AppState>, Json(req): Json<SummarizeReq>) -> Json<serde_json::Value> {
    let g = s.graph.read().await;
    let results = search(&g, &req.query, &s.synonyms, &SearchOptions {
        top_k: req.top_k.unwrap_or(5), include_failed: true, ..Default::default()
    });
    drop(g);

    if results.is_empty() {
        return Json(serde_json::json!({"error": "No search results"}));
    }

    let context: String = results.iter().enumerate().map(|(i, r)| {
        format!("{}. [{}] {} (score={:.2}, {})", i+1, r.kind, r.statement, r.score,
            if r.failed_path { "FAILED" } else { &r.status })
    }).collect::<Vec<_>>().join("\n");

    let prompt = format!(
        "다음은 '{}' 검색 결과야. 한 문단(3-5문장)으로 요약해줘.\n\
        핵심 수치, 트렌드, 주의할 점을 포함해. 한국어로.\n\n{}\n\n요약:",
        req.query, context
    );

    match call_llm(&prompt).await {
        Ok(summary) => Json(serde_json::json!({
            "query": req.query,
            "summary": summary.trim(),
            "results": results,
            "n_results": results.len(),
        })),
        Err(e) => Json(serde_json::json!({"error": e, "results": results})),
    }
}

#[derive(Deserialize)]
struct VerifyReq { claim_id: Uuid }

async fn llm_verify_claim(State(s): State<AppState>, Json(req): Json<VerifyReq>) -> Json<serde_json::Value> {
    let (claim_stmt, evidence, keywords) = {
        let g = s.graph.read().await;
        let claim = match g.get_node(&req.claim_id) {
            Some(n) if n.kind == NodeKind::Claim => n,
            _ => return Json(serde_json::json!({"error": "Claim not found"})),
        };
        let evidence_stmts: Vec<String> = g.edges.values()
            .filter(|e| e.target == req.claim_id)
            .filter_map(|e| g.get_node(&e.source))
            .filter(|n| n.kind == NodeKind::Evidence)
            .map(|n| n.statement.clone())
            .collect();
        (claim.statement.clone(), evidence_stmts, claim.keywords.clone())
    };

    let prompt = format!(
        "다음 투자 주장(Claim)의 논리적 타당성을 평가해줘.\n\n\
        주장: {}\n키워드: {}\n근거 데이터:\n{}\n\n\
        다음 형식으로 답해:\n\
        타당성: (높음/보통/낮음)\n\
        강점: (1-2줄)\n\
        약점: (1-2줄)\n\
        개선 제안: (1-2줄)\n\
        주의사항: (1줄)",
        claim_stmt,
        keywords.join(", "),
        evidence.iter().enumerate().map(|(i,f)| format!("  {}. {}", i+1, f)).collect::<Vec<_>>().join("\n")
    );

    match call_llm(&prompt).await {
        Ok(assessment) => Json(serde_json::json!({
            "claim": claim_stmt,
            "assessment": assessment.trim(),
            "n_supporting_evidence": evidence.len(),
        })),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

// ── Embedding Endpoints ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SimilarQuery { top_k: Option<usize> }

async fn similar_nodes(State(s): State<AppState>, Path(id): Path<Uuid>, Query(q): Query<SimilarQuery>) -> Json<serde_json::Value> {
    let top_k = q.top_k.unwrap_or(5);
    let g = s.graph.read().await;
    let target_emb = match g.get_node(&id).and_then(|n| n.embedding.as_ref()) {
        Some(emb) => emb.clone(),
        None => return Json(serde_json::json!({"error": "Node has no embedding", "results": []})),
    };

    let mut scored: Vec<(uuid::Uuid, f64, String)> = g.nodes.iter()
        .filter(|(nid, _)| **nid != id)
        .filter_map(|(nid, node)| {
            let emb = node.embedding.as_ref()?;
            let sim = embedding::cosine_similarity(&target_emb, emb);
            Some((*nid, sim, node.statement.clone()))
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);

    let results: Vec<serde_json::Value> = scored.iter().map(|(nid, sim, stmt)| {
        serde_json::json!({"id": nid, "similarity": sim, "statement": stmt})
    }).collect();
    Json(serde_json::json!({"results": results, "count": results.len()}))
}

async fn unrelated_nodes(State(s): State<AppState>, Path(id): Path<Uuid>, Query(q): Query<SimilarQuery>) -> Json<serde_json::Value> {
    let top_k = q.top_k.unwrap_or(5);
    let g = s.graph.read().await;
    let target_emb = match g.get_node(&id).and_then(|n| n.embedding.as_ref()) {
        Some(emb) => emb.clone(),
        None => return Json(serde_json::json!({"error": "Node has no embedding", "results": []})),
    };

    let mut scored: Vec<(uuid::Uuid, f64, String)> = g.nodes.iter()
        .filter(|(nid, _)| **nid != id)
        .filter_map(|(nid, node)| {
            let emb = node.embedding.as_ref()?;
            let sim = embedding::cosine_similarity(&target_emb, emb);
            Some((*nid, sim, node.statement.clone()))
        })
        .collect();
    // Sort ascending — least similar first
    scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);

    let results: Vec<serde_json::Value> = scored.iter().map(|(nid, sim, stmt)| {
        serde_json::json!({"id": nid, "similarity": sim, "statement": stmt})
    }).collect();
    Json(serde_json::json!({"results": results, "count": results.len()}))
}

async fn embed_all_nodes(State(s): State<AppState>) -> Json<serde_json::Value> {
    // 임베딩이 없는 노드들의 텍스트 수집
    let (texts, ids): (Vec<String>, Vec<Uuid>) = {
        let g = s.graph.read().await;
        g.nodes.iter()
            .filter(|(_, n)| n.embedding.is_none())
            .map(|(id, n)| (n.embed_text(), *id))
            .unzip()
    };

    if texts.is_empty() {
        return Json(serde_json::json!({"status": "ok", "embedded": 0, "message": "All nodes already have embeddings"}));
    }

    let total = texts.len();

    match embedding::get_embeddings_batch(&texts).await {
        Ok(embeddings) => {
            let mut g = s.graph.write().await;
            let mut count = 0usize;
            for (id, emb) in ids.iter().zip(embeddings.into_iter()) {
                if let Some(node) = g.get_node_mut(id) {
                    node.embedding = Some(emb);
                    count += 1;
                }
            }
            let _ = g.save(std::path::Path::new(DATA_DIR));
            Json(serde_json::json!({"status": "ok", "embedded": count, "total": total}))
        }
        Err(e) => {
            Json(serde_json::json!({"status": "error", "message": format!("{}", e), "total": total}))
        }
    }
}

fn node_json(n: &Node) -> serde_json::Value {
    let mut v = serde_json::json!({
        "id": n.id, "kind": format!("{:?}", n.kind), "statement": n.statement,
        "keywords": n.keywords, "status": format!("{:?}", n.status),
        "failed": n.is_failed(), "metadata": n.metadata,
        "created_at": n.created_at.to_rfc3339(),
        "updated_at": n.updated_at.to_rfc3339(),
        "has_embedding": n.embedding.is_some(),
    });
    // 임베딩 벡터는 크므로 API 응답에 포함하지 않음 (has_embedding 플래그만)
    v
}

fn edge_json(e: &Edge) -> serde_json::Value {
    serde_json::json!({
        "id": e.id, "source": e.source, "target": e.target,
        "kind": format!("{:?}", e.kind), "weight": e.weight, "note": e.note,
    })
}

fn parse_node_kind(s: &str) -> NodeKind {
    match s {
        "Claim" | "claim" => NodeKind::Claim,
        "Reason" | "reason" => NodeKind::Reason,
        "Evidence" | "evidence" => NodeKind::Evidence,
        "Experiment" | "experiment" => NodeKind::Experiment,
        _ => NodeKind::Claim,
    }
}

fn parse_node_status(s: &str) -> NodeStatus {
    match s {
        "Draft" | "draft" => NodeStatus::Draft,
        "Active" | "active" => NodeStatus::Active,
        "Accepted" | "accepted" => NodeStatus::Accepted,
        "Rejected" | "rejected" => NodeStatus::Rejected,
        "Weakened" | "weakened" => NodeStatus::Weakened,
        _ => NodeStatus::Draft,
    }
}

fn parse_edge_kind(s: &str) -> EdgeKind {
    match s {
        "support" | "Support" => EdgeKind::Support,
        "rebut" | "Rebut" => EdgeKind::Rebut,
        "depends_on" | "DependsOn" => EdgeKind::DependsOn,
        "contradicts" | "Contradicts" => EdgeKind::Contradicts,
        "refines" | "Refines" => EdgeKind::Refines,
        _ => EdgeKind::DerivedFrom,
    }
}

// ── Causal Chain + Temporal Health ───────────────────────────────────────────

async fn causal_chains(State(s): State<AppState>, Path(id): Path<Uuid>) -> Json<serde_json::Value> {
    let g = s.graph.read().await;
    let chains = g.causal_chains(&id, 5);
    Json(serde_json::json!({"chains": chains, "count": chains.len()}))
}

async fn temporal_health(State(s): State<AppState>) -> Json<serde_json::Value> {
    let g = s.graph.read().await;
    let statuses = g.temporal_health();
    let overdue = statuses.iter().filter(|s| s.urgency == "OVERDUE").count();
    let soon = statuses.iter().filter(|s| s.urgency == "SOON").count();
    Json(serde_json::json!({
        "statuses": statuses,
        "count": statuses.len(),
        "overdue": overdue,
        "soon": soon,
    }))
}

async fn impact_analysis(State(s): State<AppState>, Path(id): Path<Uuid>) -> Json<serde_json::Value> {
    let g = s.graph.read().await;
    let impacted = g.impact_analysis(&id);
    let details: Vec<serde_json::Value> = impacted.iter().filter_map(|nid| {
        g.get_node(nid).map(|n| serde_json::json!({
            "id": n.id, "statement": n.statement, "kind": format!("{:?}", n.kind),
        }))
    }).collect();
    Json(serde_json::json!({"impacted": details, "count": details.len()}))
}

async fn dependency_tree(State(s): State<AppState>, Path(id): Path<Uuid>) -> Json<serde_json::Value> {
    let g = s.graph.read().await;
    let deps = g.dependency_tree(&id);
    let details: Vec<serde_json::Value> = deps.iter().filter_map(|(nid, depth, failed)| {
        g.get_node(nid).map(|n| serde_json::json!({
            "id": n.id, "statement": n.statement, "depth": depth,
            "kind": format!("{:?}", n.kind), "failed": failed,
        }))
    }).collect();
    let has_failed_dep = deps.iter().any(|(_, _, f)| *f);
    Json(serde_json::json!({"dependencies": details, "count": details.len(), "has_failed_dependency": has_failed_dep}))
}

// ── Edge Propagation Handlers ───────────────────────────────────────────

/// 실험 verdict 후 Reason/Claim 상태 전파
async fn propagate_verdict(State(s): State<AppState>, Path(experiment_id): Path<Uuid>) -> Json<serde_json::Value> {
    let mut g = s.graph.write().await;
    match g.propagate_verdict(&experiment_id) {
        Some(result) => {
            let _ = g.save(std::path::Path::new(DATA_DIR));
            Json(serde_json::json!(result))
        }
        None => Json(serde_json::json!({"error": "experiment not found or no parent reason"})),
    }
}

/// 새 Claim의 DependsOn 자동 감지
async fn detect_claim_dependencies(State(s): State<AppState>, Path(claim_id): Path<Uuid>) -> Json<serde_json::Value> {
    let mut g = s.graph.write().await;
    let results = g.detect_claim_dependencies(&claim_id);
    if !results.is_empty() {
        let _ = g.save(std::path::Path::new(DATA_DIR));
    }
    Json(serde_json::json!({"dependencies": results, "count": results.len()}))
}

/// Knowledge 승격 시 관계 엣지 자동 생성
async fn on_accept(State(s): State<AppState>, Path(claim_id): Path<Uuid>) -> Json<serde_json::Value> {
    let mut g = s.graph.write().await;
    let relations = g.on_accept(&claim_id);
    if !relations.is_empty() {
        let _ = g.save(std::path::Path::new(DATA_DIR));
    }
    Json(serde_json::json!({"relations": relations, "count": relations.len()}))
}
