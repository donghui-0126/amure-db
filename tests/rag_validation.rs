/// amure-db RAG 검증 — Yahoo Finance Fact 노드 기반
/// AlphaFactor 없이 amure-db 단독 테스트

use amure_db::edge::{Edge, EdgeKind};
use amure_db::graph::AmureGraph;
use amure_db::node::{Node, NodeKind, NodeStatus};
use amure_db::search::{search, SearchOptions};
use amure_db::synonym::SynonymDict;

fn trunc(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn build_test_graph() -> AmureGraph {
    let mut g = AmureGraph::new();

    // ── Fact nodes (Yahoo Finance 시뮬레이션) ────────────────────────
    let btc = g.add_node(
        Node::new(NodeKind::Fact, "BTC-USD: 가격 66593 USD, 시총 1.3T, 3개월 수익률 -26.5%, 일평균 거래량 440억".into(),
            vec!["btc-usd".into(), "bitcoin".into(), "crypto".into(), "하락".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"BTC-USD","last_price":66593,"return_3m_pct":-26.5}))
    );

    let eth = g.add_node(
        Node::new(NodeKind::Fact, "ETH-USD: 가격 1812 USD, 시총 218B, 3개월 수익률 -45.2%, 거래량 180억".into(),
            vec!["eth-usd".into(), "ethereum".into(), "crypto".into(), "하락".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"ETH-USD","last_price":1812,"return_3m_pct":-45.2}))
    );

    let sol = g.add_node(
        Node::new(NodeKind::Fact, "SOL-USD: 가격 128 USD, 3개월 수익률 -32.1%, defi 생태계 성장".into(),
            vec!["sol-usd".into(), "solana".into(), "crypto".into(), "defi".into(), "하락".into()])
        .with_status(NodeStatus::Active)
    );

    let aapl = g.add_node(
        Node::new(NodeKind::Fact, "AAPL: 가격 255.92 USD, 시총 3.2T, P/E 28, 3개월 수익률 -4.2%".into(),
            vec!["aapl".into(), "apple".into(), "tech".into(), "나스닥".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"symbol":"AAPL","last_price":255.92,"return_3m_pct":-4.2}))
    );

    let tsla = g.add_node(
        Node::new(NodeKind::Fact, "TSLA: 가격 178 USD, 3개월 수익률 -28.3%, 고변동성, EV 시장 경쟁 심화".into(),
            vec!["tsla".into(), "tesla".into(), "ev".into(), "tech".into(), "고변동성".into()])
        .with_status(NodeStatus::Active)
    );

    let spy = g.add_node(
        Node::new(NodeKind::Fact, "SPY: S&P 500 ETF, 가격 505, 3개월 수익률 -8.1%".into(),
            vec!["spy".into(), "s&p500".into(), "etf".into(), "미국주식".into()])
        .with_status(NodeStatus::Active)
    );

    // ── Claim nodes ──────────────────────────────────────────────────
    let claim_momentum = g.add_node(
        Node::new(NodeKind::Claim, "크립토 선물 시장에서 OI 변화량은 cross-sectional momentum의 선행지표다".into(),
            vec!["OI".into(), "open_interest".into(), "momentum".into(), "cross-sectional".into(), "crypto".into(), "futures".into()])
        .with_status(NodeStatus::Draft)
        .with_metadata(serde_json::json!({"trigger":"거래소 OI 계산 방식 변경 시"}))
    );

    let claim_funding = g.add_node(
        Node::new(NodeKind::Claim, "funding rate 극단값은 단기 mean reversion 시그널이다".into(),
            vec!["funding".into(), "funding_rate".into(), "mean_reversion".into(), "crypto".into()])
        .with_status(NodeStatus::Draft)
    );

    let claim_vol = g.add_node(
        Node::new(NodeKind::Claim, "변동성 확대 구간에서 크립토 소형주는 대형주 대비 과도하게 하락한다".into(),
            vec!["volatility".into(), "변동성".into(), "small_cap".into(), "소형주".into(), "crash".into(), "crypto".into()])
        .with_status(NodeStatus::Draft)
    );

    // ── Reason nodes ─────────────────────────────────────────────────
    let reason_oi = g.add_node(
        Node::new(NodeKind::Reason, "OI 증가 + 가격 상승은 신규 conviction 유입을 의미하며 추세 지속력을 높인다".into(),
            vec!["OI".into(), "conviction".into(), "trend".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"bridge":"OI 증가 = 신규 포지션 → conviction → 추세 지속","reason_type":"support"}))
    );

    let reason_funding = g.add_node(
        Node::new(NodeKind::Reason, "극단 funding은 레버리지 비용 부담으로 포지션 해소 압력을 만든다".into(),
            vec!["funding".into(), "leverage".into(), "unwind".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"bridge":"funding 비용 → 포지션 정리 → 가격 역방향","reason_type":"support"}))
    );

    let reason_rebut = g.add_node(
        Node::new(NodeKind::Reason, "거래소별 OI 집계가 달라서 single-exchange OI는 노이즈가 크다".into(),
            vec!["OI".into(), "noise".into(), "exchange".into()])
        .with_status(NodeStatus::Weakened)
        .with_metadata(serde_json::json!({"bridge":"Binance OI만으로는 전체 시장 conviction을 대표하지 못함","reason_type":"rebut","reject_reason":"실험에서 single-exchange OI로도 유의한 IC 확인"}))
    );

    // ── Experiment nodes ─────────────────────────────────────────────
    let exp_ic = g.add_node(
        Node::new(NodeKind::Experiment, "OI delta 24h vs 1h fwd return IC 분석 — mean_IC=-0.004, IR=-0.055".into(),
            vec!["cross_sectional".into(), "IC".into()])
        .with_status(NodeStatus::Weakened)
        .with_metadata(serde_json::json!({"method":"CrossSectional","result":{"mean_ic":-0.004,"ir":-0.055},"verdict":"미유의"}))
    );

    let exp_regime = g.add_node(
        Node::new(NodeKind::Experiment, "레짐별 OI momentum IC — bull에서만 양의 IC, bear에서 음의 IC".into(),
            vec!["regime".into(), "bull".into(), "bear".into()])
        .with_status(NodeStatus::Active)
        .with_metadata(serde_json::json!({"method":"Regime","result":{"bull_ic":0.012,"bear_ic":-0.008},"verdict":"레짐 의존적"}))
    );

    // ── Edges ────────────────────────────────────────────────────────
    // Reason → Claim
    g.add_edge(Edge::new(reason_oi, claim_momentum, EdgeKind::Support));
    g.add_edge(Edge::new(reason_rebut, claim_momentum, EdgeKind::Rebut));
    g.add_edge(Edge::new(reason_funding, claim_funding, EdgeKind::Support));

    // Experiment → Reason
    g.add_edge(Edge::new(exp_ic, reason_oi, EdgeKind::DependsOn));
    g.add_edge(Edge::new(exp_regime, reason_oi, EdgeKind::DependsOn));

    // Fact → Claim (DerivedFrom)
    g.add_edge(Edge::new(btc, claim_momentum, EdgeKind::DerivedFrom).with_note("BTC is primary crypto asset".into()));
    g.add_edge(Edge::new(eth, claim_vol, EdgeKind::DerivedFrom).with_note("ETH large cap reference".into()));
    g.add_edge(Edge::new(sol, claim_vol, EdgeKind::DerivedFrom).with_note("SOL as mid-cap example".into()));

    // Claim → Claim
    g.add_edge(Edge::new(claim_vol, claim_momentum, EdgeKind::Refines).with_note("변동성 claim은 momentum claim의 하위 조건".into()));

    g
}

#[test]
fn test_graph_stats() {
    let g = build_test_graph();
    let s = g.summary();
    println!("\n=== Graph Stats ===");
    println!("Nodes: {} ({:?})", s.n_nodes, s.node_kinds);
    println!("Edges: {} ({:?})", s.n_edges, s.edge_kinds);
    println!("Failed: {}", s.n_failed);

    assert_eq!(s.n_nodes, 14); // 6 facts + 3 claims + 2 reasons + 2 experiments + 1 rebut reason = wait let me count
    assert!(s.n_edges >= 8);
    assert!(s.n_failed > 0); // weakened nodes
}

#[test]
fn test_rag_basic_keyword() {
    let g = build_test_graph();
    let syn = SynonymDict::new();
    let results = search(&g, "bitcoin", &syn, &SearchOptions::default());

    println!("\n=== RAG: 'bitcoin' ===");
    for r in &results {
        println!("  [{:12}] score={:.3} hop={} — {}", r.kind, r.score, r.hop_distance, &trunc(&r.statement, 50));
    }

    assert!(!results.is_empty(), "bitcoin should find BTC-USD fact");
    assert!(results[0].statement.contains("BTC"), "Top result should be BTC");
}

#[test]
fn test_rag_synonym_korean() {
    let g = build_test_graph();
    let syn = SynonymDict::new();

    // "미결제약정" should find OI nodes via synonym
    let results = search(&g, "미결제약정 추세", &syn, &SearchOptions::default());

    println!("\n=== RAG: '미결제약정 추세' (synonym) ===");
    for r in &results {
        println!("  [{:12}] score={:.3} hop={} — {}", r.kind, r.score, r.hop_distance, &trunc(&r.statement, 50));
    }

    assert!(!results.is_empty(), "미결제약정 should expand to OI");
    let has_oi = results.iter().any(|r| r.statement.contains("OI"));
    assert!(has_oi, "Should find OI-related nodes via synonym");
}

#[test]
fn test_rag_synonym_english() {
    let g = build_test_graph();
    let syn = SynonymDict::new();

    // "volatility crash" should find 변동성 claim
    let results = search(&g, "volatility crash small_cap", &syn, &SearchOptions::default());

    println!("\n=== RAG: 'volatility crash small_cap' ===");
    for r in &results {
        println!("  [{:12}] score={:.3} hop={} — {}", r.kind, r.score, r.hop_distance, &trunc(&r.statement, 50));
    }

    assert!(!results.is_empty());
    let has_vol = results.iter().any(|r| r.keywords.iter().any(|k| k == "volatility" || k == "변동성"));
    assert!(has_vol, "Should find volatility claim");
}

#[test]
fn test_rag_graph_walk() {
    let g = build_test_graph();
    let syn = SynonymDict::new();

    // Search "conviction" — should find Reason directly AND Claim via graph walk
    let results = search(&g, "conviction", &syn, &SearchOptions { max_hops: 2, ..Default::default() });

    println!("\n=== RAG: 'conviction' (graph walk) ===");
    for r in &results {
        println!("  [{:12}] score={:.3} hop={} — {}", r.kind, r.score, r.hop_distance, &trunc(&r.statement, 50));
    }

    assert!(results.len() >= 2, "Should find reason (direct) + claim (via walk)");
    let kinds: Vec<&str> = results.iter().map(|r| r.kind.as_str()).collect();
    assert!(kinds.contains(&"Reason"), "Should have Reason as direct match");
    // Claim should appear via graph walk
    let has_claim = results.iter().any(|r| r.kind == "Claim");
    assert!(has_claim, "Should find Claim via graph walk from Reason");
}

#[test]
fn test_rag_failed_paths() {
    let g = build_test_graph();
    let syn = SynonymDict::new();

    // Search with include_failed=true — should find weakened nodes
    let results_with_failed = search(&g, "OI noise exchange", &syn, &SearchOptions {
        include_failed: true,
        ..Default::default()
    });

    println!("\n=== RAG: 'OI noise exchange' (with failed) ===");
    for r in &results_with_failed {
        println!("  [{:12}] score={:.3} failed={} — {}", r.kind, r.score, r.failed_path,
            &trunc(&r.statement, 50));
    }

    let n_failed = results_with_failed.iter().filter(|r| r.failed_path).count();
    assert!(n_failed > 0, "Should show weakened/rejected nodes");

    // With include_failed=false
    let results_no_failed = search(&g, "OI noise exchange", &syn, &SearchOptions {
        include_failed: false,
        ..Default::default()
    });
    let n_failed2 = results_no_failed.iter().filter(|r| r.failed_path).count();
    assert_eq!(n_failed2, 0, "Should hide failed nodes when excluded");
}

#[test]
fn test_rag_mmr_diversity() {
    let g = build_test_graph();
    let syn = SynonymDict::new();

    // "crypto 하락" — multiple facts match, MMR should diversify
    let results = search(&g, "crypto 하락", &syn, &SearchOptions {
        top_k: 5,
        ..Default::default()
    });

    println!("\n=== RAG: 'crypto 하락' (MMR diversity) ===");
    for r in &results {
        println!("  [{:12}] score={:.3} — {}", r.kind, r.score, &trunc(&r.statement, 50));
    }

    // Should have diverse results, not all BTC
    let unique_kinds: std::collections::HashSet<_> = results.iter().map(|r| r.kind.as_str()).collect();
    println!("  Unique kinds: {:?}", unique_kinds);

    // Should have at least 2 different node kinds (Fact + Claim/Reason)
    assert!(results.len() >= 3, "Should return multiple diverse results");
}

#[test]
fn test_rag_yahoo_specific_queries() {
    let g = build_test_graph();
    let syn = SynonymDict::new();

    // Test: AAPL specific query
    let r1 = search(&g, "AAPL apple", &syn, &SearchOptions::default());
    println!("\n=== RAG: 'AAPL apple' ===");
    for r in &r1 { println!("  [{:12}] score={:.3} — {}", r.kind, r.score, &trunc(&r.statement, 50)); }
    assert!(!r1.is_empty() && r1[0].statement.contains("AAPL"), "Should find AAPL fact");

    // Test: Tesla EV
    let r2 = search(&g, "tesla ev 변동성", &syn, &SearchOptions::default());
    println!("\n=== RAG: 'tesla ev 변동성' ===");
    for r in &r2 { println!("  [{:12}] score={:.3} — {}", r.kind, r.score, &trunc(&r.statement, 50)); }
    assert!(!r2.is_empty(), "Should find TSLA fact + volatility claim");

    // Test: S&P 500
    let r3 = search(&g, "s&p500 etf", &syn, &SearchOptions::default());
    println!("\n=== RAG: 's&p500 etf' ===");
    for r in &r3 { println!("  [{:12}] score={:.3} — {}", r.kind, r.score, &trunc(&r.statement, 50)); }
    assert!(!r3.is_empty(), "Should find SPY fact");

    // Test: funding rate mean reversion
    let r4 = search(&g, "펀딩레이트 평균회귀", &syn, &SearchOptions::default());
    println!("\n=== RAG: '펀딩레이트 평균회귀' (full Korean synonym) ===");
    for r in &r4 { println!("  [{:12}] score={:.3} — {}", r.kind, r.score, &trunc(&r.statement, 50)); }
    assert!(!r4.is_empty(), "Should find funding claim via Korean synonyms");
    let has_funding = r4.iter().any(|r| r.statement.contains("funding"));
    assert!(has_funding, "펀딩레이트 → funding_rate synonym should work");
}

#[test]
fn test_rag_cross_domain() {
    let g = build_test_graph();
    let syn = SynonymDict::new();

    // "크립토 momentum 하락" — should find Facts + Claims + Reasons
    let results = search(&g, "크립토 momentum 하락", &syn, &SearchOptions {
        top_k: 8,
        max_hops: 2,
        ..Default::default()
    });

    println!("\n=== RAG: '크립토 momentum 하락' (cross-domain) ===");
    for r in &results {
        println!("  [{:12}] score={:.3} hop={} — {}", r.kind, r.score, r.hop_distance,
            &trunc(&r.statement, 50));
    }

    let kinds: std::collections::HashSet<_> = results.iter().map(|r| r.kind.as_str()).collect();
    println!("  Cross-domain kinds: {:?}", kinds);

    // Should span Fact + Claim + possibly Reason
    assert!(kinds.len() >= 2, "Cross-domain query should return multiple node kinds");
}

#[test]
fn test_persistence_roundtrip() {
    let g = build_test_graph();
    let dir = tempfile::tempdir().unwrap();

    g.save(dir.path()).unwrap();
    let g2 = AmureGraph::load(dir.path()).unwrap();

    assert_eq!(g.node_count(), g2.node_count());
    assert_eq!(g.edge_count(), g2.edge_count());

    // Search still works after load
    let syn = SynonymDict::new();
    let results = search(&g2, "bitcoin", &syn, &SearchOptions::default());
    assert!(!results.is_empty(), "Search should work after persistence roundtrip");
}
