/// amure-db v2 standalone server
/// Port 8081 — graph API + dashboard
/// Single owner of graph data. AlphaFactor calls via HTTP.

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
use amure_db::node::{Node, NodeStatus, Experiment, ExperimentKind, Verdict};
use amure_db::edge::{Edge, EdgeKind};
use amure_db::search::{search, search_balanced, SearchOptions};
use amure_db::embedding;

const DATA_DIR: &str = "data/amure_graph";
const DASHBOARD: &str = include_str!("../dashboard.html");

#[derive(Clone)]
struct AppState {
    graph: Arc<RwLock<AmureGraph>>,
}

#[tokio::main]
async fn main() {
    // .env 파일에서 환경변수 로드
    dotenvy::dotenv().ok();

    let graph = if std::path::Path::new(DATA_DIR).join("nodes.json").exists() {
        AmureGraph::load(std::path::Path::new(DATA_DIR)).unwrap_or_default()
    } else {
        AmureGraph::new()
    };
    println!("amure-db loaded: {} nodes, {} edges", graph.node_count(), graph.edge_count());

    let state = AppState {
        graph: Arc::new(RwLock::new(graph)),
    };

    let app = axum::Router::new()
        // Dashboard
        .route("/", get(dashboard))
        // Core graph CRUD
        .route("/api/graph/all", get(graph_all))
        .route("/api/graph/summary", get(graph_summary))
        .route("/api/graph/search", get(graph_search))
        .route("/api/graph/search/balanced", get(graph_search_balanced))
        .route("/api/graph/node/{id}", get(graph_node))
        .route("/api/graph/node/{id}", delete(delete_node))
        .route("/api/graph/node", post(create_node))
        .route("/api/graph/node/{id}", patch(update_node))
        .route("/api/graph/edge", post(create_edge))
        .route("/api/graph/edge/{id}", delete(delete_edge))
        .route("/api/graph/walk/{id}", get(graph_walk))
        .route("/api/graph/subgraph/{id}", get(graph_subgraph))
        // Experiment endpoints
        .route("/api/graph/node/{id}/experiments", post(add_experiment))
        .route("/api/graph/node/{id}/experiments", get(list_experiments))
        // Embedding endpoints
        .route("/api/graph/similar/{id}", get(similar_nodes))
        .route("/api/graph/unrelated/{id}", get(unrelated_nodes))
        .route("/api/graph/embed-all", post(embed_all_nodes))
        // Save
        .route("/api/save", post(save_graph))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = "0.0.0.0:8081";
    println!("amure-db v2 server: http://localhost:8081");
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
struct SearchQuery {
    q: Option<String>,
    top_k: Option<usize>,
    include_decline: Option<bool>,
    status: Option<String>,
    accept: Option<usize>,
    decline: Option<usize>,
}

async fn graph_search(State(s): State<AppState>, Query(q): Query<SearchQuery>) -> Json<serde_json::Value> {
    let query = q.q.unwrap_or_default();
    if query.is_empty() {
        let g = s.graph.read().await;
        let nodes: Vec<serde_json::Value> = g.nodes.values().map(node_json).collect();
        return Json(serde_json::json!({"results": nodes, "count": nodes.len()}));
    }

    let query_embedding = embedding::get_embedding(&query).await.ok();

    // Status pool search: accept=3&decline=2
    let status_quotas: Vec<(&str, usize)> = [
        ("Accept", q.accept),
        ("Decline", q.decline),
    ].iter().filter_map(|(s, n)| n.map(|n| (*s, n))).collect();

    let g = s.graph.read().await;

    if !status_quotas.is_empty() {
        let mut all_results = Vec::new();
        for (status, count) in &status_quotas {
            let opts = SearchOptions {
                top_k: *count,
                include_decline: true,
                status_filter: Some(status.to_string()),
                ..Default::default()
            };
            let results = search(&g, &opts, query_embedding.as_deref());
            all_results.extend(results);
        }
        return Json(serde_json::json!({"results": all_results, "count": all_results.len(), "mode": "pool"}));
    }

    // 일반 검색
    let opts = SearchOptions {
        top_k: q.top_k.unwrap_or(10),
        include_decline: q.include_decline.unwrap_or(false),
        status_filter: q.status,
        ..Default::default()
    };
    let results = search(&g, &opts, query_embedding.as_deref());
    Json(serde_json::json!({"results": results, "count": results.len()}))
}

// ── GET /api/graph/search/balanced?q=&n= ──────────────────────────────────

#[derive(Deserialize)]
struct BalancedSearchQuery {
    q: Option<String>,
    n: Option<usize>,
}

async fn graph_search_balanced(State(s): State<AppState>, Query(q): Query<BalancedSearchQuery>) -> Json<serde_json::Value> {
    let query = q.q.unwrap_or_default();
    if query.is_empty() {
        return Json(serde_json::json!({"results": [], "count": 0}));
    }

    let query_embedding = embedding::get_embedding(&query).await.ok();
    let n = q.n.unwrap_or(5);

    let g = s.graph.read().await;
    let results = search_balanced(&g, n, query_embedding.as_deref());
    Json(serde_json::json!({"results": results, "count": results.len(), "mode": "balanced"}))
}

// ── GET /api/graph/node/{id} ──────────────────────────────────────────────

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

// ── POST /api/graph/node — create hypothesis ──────────────────────────────

#[derive(Deserialize)]
struct CreateNodeReq {
    statement: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default, rename = "abstract_", alias = "abstract")]
    abstract_: Option<String>,
    #[serde(default)]
    discussion: Option<String>,
}

async fn create_node(State(s): State<AppState>, Json(req): Json<CreateNodeReq>) -> Json<serde_json::Value> {
    let status = req.status.as_deref().map(parse_node_status).unwrap_or(NodeStatus::Draft);
    let mut node = Node::new(req.statement);
    node.status = status;
    if let Some(ref abs) = req.abstract_ {
        node.abstract_ = abs.clone();
    }
    if let Some(ref disc) = req.discussion {
        node.discussion = disc.clone();
    }

    let embed_text = node.embed_text();
    let id = node.id;

    let mut g = s.graph.write().await;
    g.add_node(node);
    let _ = g.save(std::path::Path::new(DATA_DIR));
    drop(g);

    // 임베딩 비동기 계산
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

// ── PATCH /api/graph/node/{id} — update node ─────────────────────────────

#[derive(Deserialize)]
struct UpdateNodeReq {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    statement: Option<String>,
    #[serde(default, rename = "abstract_", alias = "abstract")]
    abstract_: Option<String>,
    #[serde(default)]
    discussion: Option<String>,
}

async fn update_node(State(s): State<AppState>, Path(id): Path<Uuid>, Json(req): Json<UpdateNodeReq>) -> Json<serde_json::Value> {
    let mut g = s.graph.write().await;
    match g.get_node_mut(&id) {
        Some(node) => {
            if let Some(ref status) = req.status {
                node.status = parse_node_status(status);
            }
            if let Some(ref stmt) = req.statement {
                node.statement = stmt.clone();
            }
            if let Some(ref abs) = req.abstract_ {
                node.abstract_ = abs.clone();
            }
            if let Some(ref disc) = req.discussion {
                node.discussion = disc.clone();
            }
            node.updated_at = chrono::Utc::now();
            let _ = g.save(std::path::Path::new(DATA_DIR));
            Json(serde_json::json!({"status": "updated", "id": id}))
        }
        None => Json(serde_json::json!({"error": "Not found"})),
    }
}

// ── POST /api/graph/edge — create edge (reason required) ─────────────────

#[derive(Deserialize)]
struct CreateEdgeReq {
    source: Uuid,
    target: Uuid,
    kind: String,
    reason: String,
}

async fn create_edge(State(s): State<AppState>, Json(req): Json<CreateEdgeReq>) -> Json<serde_json::Value> {
    if req.reason.trim().is_empty() {
        return Json(serde_json::json!({"error": "reason is required"}));
    }
    let kind = parse_edge_kind(&req.kind);
    let mut g = s.graph.write().await;

    // 정방향 엣지
    let edge = Edge::new(req.source, req.target, kind, req.reason.clone());
    let id = g.add_edge(edge);

    // 역방향 엣지 자동 생성
    let reverse_kind = match kind {
        EdgeKind::Subset => EdgeKind::Superset,
        EdgeKind::Superset => EdgeKind::Subset,
        EdgeKind::Reference => EdgeKind::Reference,
        EdgeKind::Orthogonal => EdgeKind::Orthogonal,
    };
    let reverse_edge = Edge::new(req.target, req.source, reverse_kind, req.reason);
    let reverse_id = g.add_edge(reverse_edge);

    let _ = g.save(std::path::Path::new(DATA_DIR));
    Json(serde_json::json!({"status": "created", "id": id, "reverse_id": reverse_id}))
}

// ── DELETE /api/graph/edge/{id} ───────────────────────────────────────────

async fn delete_edge(State(s): State<AppState>, Path(id): Path<Uuid>) -> Json<serde_json::Value> {
    let mut g = s.graph.write().await;
    if g.remove_edge(&id).is_some() {
        let _ = g.save(std::path::Path::new(DATA_DIR));
        Json(serde_json::json!({"status": "deleted"}))
    } else {
        Json(serde_json::json!({"error": "Not found"}))
    }
}

// ── GET /api/graph/walk/{id}?hops=1&exclude_orthogonal=false ─────────────

#[derive(Deserialize, Default)]
struct WalkQuery {
    hops: Option<usize>,
    exclude_orthogonal: Option<bool>,
}

async fn graph_walk(State(s): State<AppState>, Path(id): Path<Uuid>, Query(q): Query<WalkQuery>) -> Json<serde_json::Value> {
    let max_hops = q.hops.unwrap_or(1);
    let exclude_orthogonal = q.exclude_orthogonal.unwrap_or(false);
    let g = s.graph.read().await;
    if g.get_node(&id).is_none() {
        return Json(serde_json::json!({"error": "Node not found"}));
    }
    let walked = if exclude_orthogonal {
        g.walk_exclude_orthogonal(&id, max_hops)
    } else {
        g.walk(&id, max_hops, None)
    };
    let nodes: Vec<serde_json::Value> = walked.iter().filter_map(|(nid, depth)| {
        g.get_node(nid).map(|n| {
            let mut nj = node_json(n);
            nj.as_object_mut().unwrap().insert("depth".into(), serde_json::json!(depth));
            nj
        })
    }).collect();
    Json(serde_json::json!({"start": id, "max_hops": max_hops, "exclude_orthogonal": exclude_orthogonal, "nodes": nodes, "count": nodes.len()}))
}

// ── GET /api/graph/subgraph/{id} ──────────────────────────────────────────

async fn graph_subgraph(State(s): State<AppState>, Path(id): Path<Uuid>) -> Json<serde_json::Value> {
    let g = s.graph.read().await;
    if g.get_node(&id).is_none() {
        return Json(serde_json::json!({"error": "Node not found"}));
    }
    let walked = g.walk(&id, 10, None);
    let node_ids: Vec<Uuid> = walked.iter().map(|(nid, _)| *nid).collect();
    let (nodes, edges) = g.subgraph(&node_ids);

    let node_list: Vec<serde_json::Value> = nodes.iter().map(|n| node_json(n)).collect();
    let edge_list: Vec<serde_json::Value> = edges.iter().map(|e| edge_json(e)).collect();

    Json(serde_json::json!({
        "root": id,
        "nodes": node_list,
        "edges": edge_list,
        "n_nodes": node_list.len(),
        "n_edges": edge_list.len(),
    }))
}

// ── POST /api/graph/node/{id}/experiments — add experiment ────────────────

#[derive(Deserialize)]
struct AddExperimentReq {
    kind: String,
    target: String,
    result: serde_json::Value,
    verdict: String,
    #[serde(default)]
    note: Option<String>,
}

async fn add_experiment(State(s): State<AppState>, Path(id): Path<Uuid>, Json(req): Json<AddExperimentReq>) -> Json<serde_json::Value> {
    let exp_kind = parse_experiment_kind(&req.kind);
    let verdict = parse_verdict(&req.verdict);

    let experiment = Experiment {
        id: Uuid::new_v4(),
        kind: exp_kind,
        target: req.target,
        result: req.result,
        verdict,
        note: req.note,
    };
    let exp_id = experiment.id;

    let mut g = s.graph.write().await;
    match g.get_node_mut(&id) {
        Some(node) => {
            node.experiments.push(experiment);
            node.updated_at = chrono::Utc::now();
            let _ = g.save(std::path::Path::new(DATA_DIR));
            Json(serde_json::json!({"status": "added", "experiment_id": exp_id, "node_id": id}))
        }
        None => Json(serde_json::json!({"error": "Node not found"})),
    }
}

// ── GET /api/graph/node/{id}/experiments — list experiments ───────────────

async fn list_experiments(State(s): State<AppState>, Path(id): Path<Uuid>) -> Json<serde_json::Value> {
    let g = s.graph.read().await;
    match g.get_node(&id) {
        Some(node) => {
            let experiments: Vec<serde_json::Value> = node.experiments.iter().map(|exp| {
                serde_json::json!({
                    "id": exp.id,
                    "kind": format!("{:?}", exp.kind),
                    "target": exp.target,
                    "result": exp.result,
                    "verdict": format!("{:?}", exp.verdict),
                    "note": exp.note,
                })
            }).collect();
            Json(serde_json::json!({"experiments": experiments, "count": experiments.len(), "node_id": id}))
        }
        None => Json(serde_json::json!({"error": "Node not found"})),
    }
}

// ── Embedding Endpoints ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct SimilarQuery { top_k: Option<usize> }

async fn similar_nodes(State(s): State<AppState>, Path(id): Path<Uuid>, Query(q): Query<SimilarQuery>) -> Json<serde_json::Value> {
    let top_k = q.top_k.unwrap_or(5);
    let g = s.graph.read().await;
    let target_emb = match g.get_node(&id).and_then(|n| n.embedding.as_ref()) {
        Some(emb) => emb.clone(),
        None => return Json(serde_json::json!({"error": "Node has no embedding", "results": []})),
    };

    let mut scored: Vec<(Uuid, f64, String)> = g.nodes.iter()
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

    let mut scored: Vec<(Uuid, f64, String)> = g.nodes.iter()
        .filter(|(nid, _)| **nid != id)
        .filter_map(|(nid, node)| {
            let emb = node.embedding.as_ref()?;
            let sim = embedding::cosine_similarity(&target_emb, emb);
            Some((*nid, sim, node.statement.clone()))
        })
        .collect();
    scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);

    let results: Vec<serde_json::Value> = scored.iter().map(|(nid, sim, stmt)| {
        serde_json::json!({"id": nid, "similarity": sim, "statement": stmt})
    }).collect();
    Json(serde_json::json!({"results": results, "count": results.len()}))
}

async fn embed_all_nodes(State(s): State<AppState>) -> Json<serde_json::Value> {
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

// ── Save ──────────────────────────────────────────────────────────────────

async fn save_graph(State(s): State<AppState>) -> Json<serde_json::Value> {
    let g = s.graph.read().await;
    match g.save(std::path::Path::new(DATA_DIR)) {
        Ok(_) => Json(serde_json::json!({"status": "saved"})),
        Err(e) => Json(serde_json::json!({"error": e.to_string()})),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn node_json(n: &Node) -> serde_json::Value {
    let experiments: Vec<serde_json::Value> = n.experiments.iter().map(|exp| {
        serde_json::json!({
            "id": exp.id,
            "kind": format!("{:?}", exp.kind),
            "target": exp.target,
            "result": exp.result,
            "verdict": format!("{:?}", exp.verdict),
            "note": exp.note,
        })
    }).collect();

    serde_json::json!({
        "id": n.id,
        "kind": format!("{:?}", n.kind),
        "statement": n.statement,
        "abstract_": n.abstract_,
        "discussion": n.discussion,
        "status": format!("{:?}", n.status),
        "experiments": experiments,
        "n_experiments": n.experiments.len(),
        "created_at": n.created_at.to_rfc3339(),
        "updated_at": n.updated_at.to_rfc3339(),
        "has_embedding": n.embedding.is_some(),
    })
}

fn edge_json(e: &Edge) -> serde_json::Value {
    serde_json::json!({
        "id": e.id, "source": e.source, "target": e.target,
        "kind": format!("{:?}", e.kind), "reason": e.reason,
    })
}

fn parse_node_status(s: &str) -> NodeStatus {
    match s {
        "Draft" | "draft" => NodeStatus::Draft,
        "Accept" | "accept" | "Accepted" | "accepted" => NodeStatus::Accept,
        "Decline" | "decline" | "Declined" | "declined" | "Rejected" | "rejected" => NodeStatus::Decline,
        _ => NodeStatus::Draft,
    }
}

fn parse_edge_kind(s: &str) -> EdgeKind {
    match s {
        "Reference" | "reference" => EdgeKind::Reference,
        "Superset" | "superset" => EdgeKind::Superset,
        "Subset" | "subset" => EdgeKind::Subset,
        "Orthogonal" | "orthogonal" => EdgeKind::Orthogonal,
        _ => EdgeKind::Reference,
    }
}

fn parse_experiment_kind(s: &str) -> ExperimentKind {
    match s {
        "Universe" | "universe" => ExperimentKind::Universe,
        "Regime" | "regime" => ExperimentKind::Regime,
        "Temporal" | "temporal" => ExperimentKind::Temporal,
        "Combo" | "combo" => ExperimentKind::Combo,
        _ => ExperimentKind::Universe,
    }
}

fn parse_verdict(s: &str) -> Verdict {
    match s {
        "Support" | "support" => Verdict::Support,
        "Rebut" | "rebut" => Verdict::Rebut,
        _ => Verdict::Support,
    }
}
