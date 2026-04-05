/// amure-db RAG 검증 — Yahoo Finance 주식 데이터만
/// 크립토 없이, 순수 주식 Fact 노드로 RAG 성능 확인

use amure_db::edge::{Edge, EdgeKind};
use amure_db::graph::AmureGraph;
use amure_db::node::{Node, NodeKind, NodeStatus};
use amure_db::search::{search, SearchOptions};
use amure_db::synonym::SynonymDict;

fn trunc(s: &str, n: usize) -> String { s.chars().take(n).collect() }

fn build_yahoo_graph() -> AmureGraph {
    let mut g = AmureGraph::new();

    // ── 15개 종목 Fact nodes (Yahoo Finance 시뮬레이션) ──────────────
    let aapl = g.add_node(Node::new(NodeKind::Fact,
        "AAPL: Apple Inc. 가격 255.92 USD, 시총 3.2T, P/E 28.3, 3개월 -4.2%, 매출 성장 안정적".into(),
        vec!["aapl".into(), "apple".into(), "tech".into(), "나스닥".into(), "대형주".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"AAPL","sector":"Technology","price":255.92,"pe":28.3,"return_3m":-4.2})));

    let msft = g.add_node(Node::new(NodeKind::Fact,
        "MSFT: Microsoft 가격 388 USD, 시총 2.9T, P/E 32.1, 3개월 -8.5%, AI/클라우드 성장 주도".into(),
        vec!["msft".into(), "microsoft".into(), "tech".into(), "ai".into(), "클라우드".into(), "대형주".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"MSFT","sector":"Technology","price":388.0,"pe":32.1,"return_3m":-8.5})));

    let googl = g.add_node(Node::new(NodeKind::Fact,
        "GOOGL: Alphabet 가격 155 USD, 시총 1.9T, P/E 20.5, 3개월 -12.3%, 검색+AI 경쟁 심화".into(),
        vec!["googl".into(), "google".into(), "alphabet".into(), "tech".into(), "ai".into(), "광고".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"GOOGL","sector":"Technology","price":155.0,"return_3m":-12.3})));

    let amzn = g.add_node(Node::new(NodeKind::Fact,
        "AMZN: Amazon 가격 178 USD, 시총 1.85T, P/E 40.2, 3개월 -15.1%, AWS 성장 둔화 우려".into(),
        vec!["amzn".into(), "amazon".into(), "tech".into(), "ecommerce".into(), "aws".into(), "클라우드".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"AMZN","sector":"Consumer Cyclical","price":178.0,"return_3m":-15.1})));

    let tsla = g.add_node(Node::new(NodeKind::Fact,
        "TSLA: Tesla 가격 178 USD, P/E 55.8, 3개월 -28.3%, EV 시장 경쟁 심화, 마진 압박".into(),
        vec!["tsla".into(), "tesla".into(), "ev".into(), "전기차".into(), "고변동성".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"TSLA","sector":"Consumer Cyclical","price":178.0,"return_3m":-28.3})));

    let nvda = g.add_node(Node::new(NodeKind::Fact,
        "NVDA: NVIDIA 가격 108 USD, 시총 2.6T, P/E 48.2, 3개월 -22.6%, AI GPU 수요는 강하나 밸류에이션 부담".into(),
        vec!["nvda".into(), "nvidia".into(), "ai".into(), "gpu".into(), "반도체".into(), "tech".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"NVDA","sector":"Technology","price":108.0,"return_3m":-22.6})));

    let meta = g.add_node(Node::new(NodeKind::Fact,
        "META: Meta Platforms 가격 520 USD, P/E 22.1, 3개월 -18.7%, 메타버스 투자 지속, 광고 회복".into(),
        vec!["meta".into(), "facebook".into(), "tech".into(), "광고".into(), "메타버스".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"META","sector":"Technology","price":520.0,"return_3m":-18.7})));

    let jpm = g.add_node(Node::new(NodeKind::Fact,
        "JPM: JPMorgan Chase 가격 238 USD, P/E 12.5, 3개월 +2.1%, 금리 환경 수혜, 안정적 배당".into(),
        vec!["jpm".into(), "jpmorgan".into(), "은행".into(), "금융".into(), "배당".into(), "가치주".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"JPM","sector":"Financial","price":238.0,"return_3m":2.1})));

    let ko = g.add_node(Node::new(NodeKind::Fact,
        "KO: Coca-Cola 가격 62 USD, P/E 25.8, 3개월 +5.3%, 경기방어주, 안정적 배당 성장".into(),
        vec!["ko".into(), "coca-cola".into(), "음료".into(), "경기방어".into(), "배당".into(), "가치주".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"KO","sector":"Consumer Defensive","price":62.0,"return_3m":5.3})));

    let xom = g.add_node(Node::new(NodeKind::Fact,
        "XOM: Exxon Mobil 가격 108 USD, P/E 14.2, 3개월 -3.8%, 유가 하락 영향, 배당 매력".into(),
        vec!["xom".into(), "exxon".into(), "에너지".into(), "석유".into(), "배당".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"XOM","sector":"Energy","price":108.0,"return_3m":-3.8})));

    let spy = g.add_node(Node::new(NodeKind::Fact,
        "SPY: S&P 500 ETF 가격 505 USD, 3개월 -8.1%, 미국 대형주 전체 시장 지표".into(),
        vec!["spy".into(), "s&p500".into(), "etf".into(), "미국주식".into(), "인덱스".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"SPY","sector":"ETF","price":505.0,"return_3m":-8.1})));

    let qqq = g.add_node(Node::new(NodeKind::Fact,
        "QQQ: Nasdaq 100 ETF 가격 430 USD, 3개월 -12.5%, 기술주 비중 높아 변동성 큼".into(),
        vec!["qqq".into(), "nasdaq".into(), "나스닥".into(), "etf".into(), "tech".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"QQQ","sector":"ETF","price":430.0,"return_3m":-12.5})));

    let voo = g.add_node(Node::new(NodeKind::Fact,
        "VOO: Vanguard S&P 500 ETF 가격 462 USD, 3개월 -8.0%, SPY 대비 저비용".into(),
        vec!["voo".into(), "vanguard".into(), "etf".into(), "s&p500".into(), "저비용".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"VOO","sector":"ETF","price":462.0,"return_3m":-8.0})));

    let schd = g.add_node(Node::new(NodeKind::Fact,
        "SCHD: Schwab 배당 ETF 가격 78 USD, 3개월 +1.2%, 고배당 가치주 중심".into(),
        vec!["schd".into(), "배당".into(), "etf".into(), "가치주".into(), "income".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"SCHD","sector":"ETF","price":78.0,"return_3m":1.2})));

    let tlt = g.add_node(Node::new(NodeKind::Fact,
        "TLT: 미국 장기 국채 ETF 가격 88 USD, 3개월 +4.5%, 금리 인하 기대 반영".into(),
        vec!["tlt".into(), "국채".into(), "채권".into(), "etf".into(), "금리".into(), "안전자산".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"TLT","sector":"Bond","price":88.0,"return_3m":4.5})));

    // ── Claim nodes (투자 가설) ──────────────────────────────────────
    let claim_ai = g.add_node(Node::new(NodeKind::Claim,
        "AI 인프라 투자 사이클은 2026년까지 지속되며 GPU/클라우드 기업이 수혜를 받는다".into(),
        vec!["ai".into(), "gpu".into(), "클라우드".into(), "반도체".into(), "growth".into()])
        .with_metadata(serde_json::json!({"trigger":"AI capex 감소 신호 나오면 재검토"})));

    let claim_div = g.add_node(Node::new(NodeKind::Claim,
        "금리 인하 사이클에서 고배당 가치주는 성장주 대비 초과 수익을 낸다".into(),
        vec!["배당".into(), "가치주".into(), "금리".into(), "income".into(), "방어적".into()])
        .with_metadata(serde_json::json!({"trigger":"금리 인상 재개 시"})));

    let claim_tech = g.add_node(Node::new(NodeKind::Claim,
        "빅테크 밸류에이션이 P/E 30 이상이면 12개월 수익률이 시장 평균 이하다".into(),
        vec!["tech".into(), "valuation".into(), "pe".into(), "대형주".into()])
        .with_metadata(serde_json::json!({"trigger":"PE 급락 시 재검토"})));

    // ── Reason nodes ─────────────────────────────────────────────────
    let r_ai_support = g.add_node(Node::new(NodeKind::Reason,
        "NVDA/MSFT/GOOGL의 AI capex가 전년 대비 50%+ 증가하고 있어 수요는 실재한다".into(),
        vec!["ai".into(), "capex".into(), "nvda".into(), "msft".into()])
        .with_metadata(serde_json::json!({"bridge":"capex 증가 = 실수요 → 매출 성장 → 주가 지지","reason_type":"support"})));

    let r_ai_rebut = g.add_node(Node::new(NodeKind::Reason,
        "AI 투자 회수율(ROI)이 불명확하고 과잉투자 우려가 커지고 있다".into(),
        vec!["ai".into(), "roi".into(), "과잉투자".into(), "버블".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"bridge":"ROI 불확실 → capex 삭감 가능 → 성장 둔화","reason_type":"rebut"})));

    let r_div_support = g.add_node(Node::new(NodeKind::Reason,
        "금리 하락 시 배당 수익률의 상대적 매력이 증가하고 채권 대체 수요가 유입된다".into(),
        vec!["금리".into(), "배당".into(), "채권".into(), "수익률".into()])
        .with_metadata(serde_json::json!({"bridge":"금리↓ → 채권 수익률↓ → 배당주로 자금 이동","reason_type":"support"})));

    // ── Edges ────────────────────────────────────────────────────────
    g.add_edge(Edge::new(r_ai_support, claim_ai, EdgeKind::Support));
    g.add_edge(Edge::new(r_ai_rebut, claim_ai, EdgeKind::Rebut));
    g.add_edge(Edge::new(r_div_support, claim_div, EdgeKind::Support));

    // Fact → Claim links
    g.add_edge(Edge::new(nvda, claim_ai, EdgeKind::DerivedFrom).with_note("NVDA is core AI GPU supplier".into()));
    g.add_edge(Edge::new(msft, claim_ai, EdgeKind::DerivedFrom).with_note("MSFT Azure AI growth".into()));
    g.add_edge(Edge::new(googl, claim_ai, EdgeKind::DerivedFrom).with_note("GOOGL AI/Cloud".into()));
    g.add_edge(Edge::new(jpm, claim_div, EdgeKind::DerivedFrom).with_note("JPM high dividend financal".into()));
    g.add_edge(Edge::new(ko, claim_div, EdgeKind::DerivedFrom).with_note("KO stable dividend".into()));
    g.add_edge(Edge::new(schd, claim_div, EdgeKind::DerivedFrom).with_note("SCHD dividend ETF".into()));
    g.add_edge(Edge::new(aapl, claim_tech, EdgeKind::DerivedFrom).with_note("AAPL high PE tech".into()));
    g.add_edge(Edge::new(meta, claim_tech, EdgeKind::DerivedFrom));

    // Claim → Claim
    g.add_edge(Edge::new(claim_tech, claim_ai, EdgeKind::Contradicts).with_note("고PE → 수익률 하락 vs AI 성장 지속".into()));
    g.add_edge(Edge::new(claim_div, claim_tech, EdgeKind::Refines).with_note("가치주 vs 성장주 관점".into()));

    g
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn test_graph_overview() {
    let g = build_yahoo_graph();
    let s = g.summary();
    println!("\n=== Yahoo Finance Graph ===");
    println!("Nodes: {} ({:?})", s.n_nodes, s.node_kinds);
    println!("Edges: {} ({:?})", s.n_edges, s.edge_kinds);
    assert_eq!(*s.node_kinds.get("Fact").unwrap_or(&0), 15);
    assert_eq!(*s.node_kinds.get("Claim").unwrap_or(&0), 3);
    assert_eq!(*s.node_kinds.get("Reason").unwrap_or(&0), 3);
    assert!(s.n_edges >= 12);
}

#[test]
fn test_search_by_ticker() {
    let g = build_yahoo_graph();
    let syn = SynonymDict::new();

    for (query, expected) in [("AAPL", "AAPL"), ("NVDA nvidia", "NVDA"), ("TSLA tesla", "TSLA"), ("JPM jpmorgan", "JPM")] {
        let r = search(&g, query, &syn, &SearchOptions::default());
        println!("\n=== '{}' ===", query);
        for x in &r { println!("  [{:8}] {:.3} — {}", x.kind, x.score, trunc(&x.statement, 50)); }
        assert!(!r.is_empty(), "{} should find results", query);
        assert!(r[0].statement.contains(expected), "Top result for {} should contain {}", query, expected);
    }
}

#[test]
fn test_search_by_sector() {
    let g = build_yahoo_graph();
    let syn = SynonymDict::new();

    // "tech ai" should find NVDA, MSFT, GOOGL + AI claim
    let r = search(&g, "tech ai", &syn, &SearchOptions { top_k: 8, ..Default::default() });
    println!("\n=== 'tech ai' (sector) ===");
    for x in &r { println!("  [{:8}] {:.3} — {}", x.kind, x.score, trunc(&x.statement, 50)); }
    let facts: Vec<_> = r.iter().filter(|x| x.kind == "Fact").collect();
    let claims: Vec<_> = r.iter().filter(|x| x.kind == "Claim").collect();
    assert!(facts.len() >= 3, "Should find multiple tech facts");
    assert!(!claims.is_empty(), "Should find AI claim via graph walk");
}

#[test]
fn test_search_korean_concept() {
    let g = build_yahoo_graph();
    let syn = SynonymDict::new();

    // "배당 가치주" — should find KO, JPM, SCHD + dividend claim
    let r = search(&g, "배당 가치주", &syn, &SearchOptions { top_k: 8, ..Default::default() });
    println!("\n=== '배당 가치주' ===");
    for x in &r { println!("  [{:8}] {:.3} — {}", x.kind, x.score, trunc(&x.statement, 50)); }
    let has_ko = r.iter().any(|x| x.statement.contains("KO") || x.statement.contains("Coca-Cola"));
    let has_claim = r.iter().any(|x| x.kind == "Claim" && x.statement.contains("배당"));
    assert!(has_ko, "Should find KO via 배당 keyword");
    assert!(has_claim, "Should find dividend claim");
}

#[test]
fn test_search_etf() {
    let g = build_yahoo_graph();
    let syn = SynonymDict::new();

    let r = search(&g, "etf 인덱스", &syn, &SearchOptions { top_k: 5, ..Default::default() });
    println!("\n=== 'etf 인덱스' ===");
    for x in &r { println!("  [{:8}] {:.3} — {}", x.kind, x.score, trunc(&x.statement, 50)); }
    assert!(r.len() >= 3, "Should find SPY, QQQ, VOO, SCHD, TLT");
}

#[test]
fn test_graph_walk_from_claim() {
    let g = build_yahoo_graph();
    let syn = SynonymDict::new();

    // Search "AI 인프라" → find AI claim → walk to NVDA, MSFT, GOOGL facts
    let r = search(&g, "AI 인프라 투자", &syn, &SearchOptions { top_k: 10, max_hops: 2, ..Default::default() });
    println!("\n=== 'AI 인프라 투자' (graph walk) ===");
    for x in &r { println!("  [{:8}] {:.3} hop={} — {}", x.kind, x.score, x.hop_distance, trunc(&x.statement, 50)); }

    let direct = r.iter().filter(|x| x.hop_distance == 0).count();
    let walked = r.iter().filter(|x| x.hop_distance > 0).count();
    println!("  Direct: {}, Walked: {}", direct, walked);
    assert!(walked > 0, "Graph walk should discover connected nodes");

    // Should find NVDA/MSFT/GOOGL via edges from AI claim
    let has_nvda = r.iter().any(|x| x.statement.contains("NVDA") || x.statement.contains("NVIDIA"));
    assert!(has_nvda, "Should discover NVDA via graph walk from AI claim");
}

#[test]
fn test_mmr_diversity_stocks() {
    let g = build_yahoo_graph();
    let syn = SynonymDict::new();

    // "tech 대형주" — should not return only AAPL-like results
    let r = search(&g, "tech 대형주", &syn, &SearchOptions { top_k: 6, ..Default::default() });
    println!("\n=== 'tech 대형주' (MMR diversity) ===");
    for x in &r { println!("  [{:8}] {:.3} — {}", x.kind, x.score, trunc(&x.statement, 50)); }

    let symbols: Vec<String> = r.iter().filter_map(|x| {
        x.statement.split(':').next().map(|s| s.trim().to_string())
    }).collect();
    let unique: std::collections::HashSet<_> = symbols.iter().collect();
    println!("  Unique symbols: {:?}", unique);
    assert!(unique.len() >= 3, "MMR should diversify across different stocks");
}

#[test]
fn test_contradicts_edge() {
    let g = build_yahoo_graph();
    let syn = SynonymDict::new();

    // "PE valuation 고평가" → should find tech PE claim + AI claim via Contradicts edge
    let r = search(&g, "PE valuation 고평가 tech", &syn, &SearchOptions { top_k: 5, max_hops: 2, ..Default::default() });
    println!("\n=== 'PE valuation 고평가' (Contradicts edge) ===");
    for x in &r { println!("  [{:8}] {:.3} hop={} — {}", x.kind, x.score, x.hop_distance, trunc(&x.statement, 50)); }

    let claims: Vec<_> = r.iter().filter(|x| x.kind == "Claim").collect();
    assert!(claims.len() >= 2, "Should find PE claim + AI claim via Contradicts edge");
}

#[test]
fn test_cross_sector_query() {
    let g = build_yahoo_graph();
    let syn = SynonymDict::new();

    // "금리 채권 안전자산" → TLT + dividend claim + JPM/KO
    let r = search(&g, "금리 채권 안전자산", &syn, &SearchOptions { top_k: 8, max_hops: 2, ..Default::default() });
    println!("\n=== '금리 채권 안전자산' (cross-sector) ===");
    for x in &r { println!("  [{:8}] {:.3} hop={} — {}", x.kind, x.score, x.hop_distance, trunc(&x.statement, 50)); }

    let has_tlt = r.iter().any(|x| x.statement.contains("TLT"));
    let has_div_claim = r.iter().any(|x| x.kind == "Claim" && x.statement.contains("배당"));
    assert!(has_tlt, "Should find TLT bond ETF");
    assert!(has_div_claim, "Should find dividend claim via graph walk from 금리");
}

#[test]
fn test_persistence_yahoo() {
    let g = build_yahoo_graph();
    let dir = tempfile::tempdir().unwrap();
    g.save(dir.path()).unwrap();

    let g2 = AmureGraph::load(dir.path()).unwrap();
    assert_eq!(g.node_count(), g2.node_count());
    assert_eq!(g.edge_count(), g2.edge_count());

    let syn = SynonymDict::new();
    let r = search(&g2, "AAPL apple", &syn, &SearchOptions::default());
    assert!(!r.is_empty() && r[0].statement.contains("AAPL"));
    println!("\n=== Persistence roundtrip: AAPL found after save/load ===");
}
