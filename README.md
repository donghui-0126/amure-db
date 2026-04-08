# amure-db

Hypothesis 기반 퀀트 지식 그래프 DB. AlphaFactor의 RAG 백엔드.

---

## Architecture

```
AlphaFactor (:8080)  →  amure-db (:8081)
                         - Hypothesis 노드 저장
                         - OpenAI embedding 기반 RAG 검색
                         - 그래프 관계 (reference/superset/subset/orthogonal)
```

### Source Files

| File | Role |
|------|------|
| `node.rs` | Hypothesis, Experiment, NodeStatus |
| `edge.rs` | EdgeKind (Reference/Superset/Subset/Orthogonal), reason 필드 |
| `graph.rs` | in-memory 그래프, BFS walk |
| `search.rs` | embedding cosine → MMR reranking, keyword fallback |
| `embedding.rs` | OpenAI text-embedding-3-small |
| `persistence.rs` | JSON atomic write |
| `server.rs` | Axum HTTP 서버 |

---

## Core Concepts

### Node: Hypothesis

모든 지식은 **Hypothesis 단일 노드**로 표현된다.

```
Hypothesis {
    id          Uuid
    statement   String              // 가설 한 문장
    abstract_   String              // 핵심 발견 한줄평
    discussion  String              // 한계, 노이즈, 추가 관찰
    status      Draft | Accept | Decline
    experiments Vec<Experiment>     // 실험 결과 내장
    embedding   Vec<f32>            // OpenAI text-embedding-3-small
}

Experiment {
    kind    Universe | Regime | Temporal | Combo
    target  String              // 심볼군 / regime 축 / 기간 등
    result  JSON                // IR, t-stat, hit_ratio 등
    verdict Support | Rebut
    note    String?
}
```

**고정 실험:**

| 종류 | 내용 | 필수 |
|------|------|------|
| Universe | 신생/비신생/메이저/알트/마이너 심볼군별 | ★ |
| Regime | ETH가격·자기자신가격·ETHOI·자기자신OI × bull/bear/sideways | ★ |
| Temporal | monthly rolling IR/t-stat, improving/decaying 추세 | ★ |
| Combo | feature/event 결합 조건부 검증 | ★★ |

**Universe strict 기준:** `|IR| > 0.1 AND |t| > 1.96` 미달 → 해당 심볼군 자동 rebut

**Regime strict 기준:** 최소 1축 유의미 필수, 방향 불일치 다수 → rebut 가중

### Edge

노드 간 관계. **모든 엣지에 reason(사유) 필수.**

```
Edge {
    source  Uuid
    target  Uuid
    kind    Reference | Superset | Subset | Orthogonal
    reason  String    // 왜 이 관계인지
}
```

| 관계 | 의미 | 1-hop context 포함 |
|------|------|-------------------|
| reference | 조건/메커니즘 교집합 | ✓ |
| superset | source가 target의 상위개념 | ✓ |
| subset | source가 target의 부분집합 | ✓ |
| orthogonal | 완전히 다른 영역 | ✗ |

---

## V2 Pipeline (AlphaFactor 연동)

```
[User: idea 입력]
      │
      ▼
   RAG SEARCH
  - GET /api/graph/search/balanced?q=&n=
  - accept N개 + decline N개 균형
  - root node 목록 + 각 1-hop 기존 엣지 로드
      │
      ▼
  GRAPH TO TEXT GATE
  ┌─────────────────────────────────────────────────────────┐
  │ Step 1. root node 기존 엣지 텍스트화 (rule-based)       │
  │                                                         │
  │   A(accept): {abstract}                                 │
  │     →[superset]  B: {abstract}    ← A-B 기존 엣지       │
  │     →[reference] C: {abstract}    ← A-C 기존 엣지       │
  │                                                         │
  │   X(accept): {abstract} →[orthogonal]                   │
  │     (1-hop 없음)                                        │
  │                                                         │
  │ Step 2. idea → 각 root node 관계 + 사유 판단 (LLM)     │
  │                                                         │
  │   반환:                                                 │
  │   { node_id: A, relation: "reference",                  │
  │     reason: "BTC 급등 + 선물 과잉 메커니즘 공유" }      │
  │   { node_id: X, relation: "orthogonal",                 │
  │     reason: "대상 심볼군과 조건이 완전히 다름" }         │
  │                                                         │
  │   → 관계 + 사유 메모리 유지 (엣지 저장 시 재사용)       │
  └─────────────────────────────────────────────────────────┘
      │
      ▼
  GENERATE (LLM)
  - graph-to-text context로 Hypothesis statement 작성
  - pick 목록 반환 (실제 참고한 root node ID)
  - POST /api/graph/node  → Hypothesis 생성 (status: draft)
      │
      ▼
  EXECUTE (Julia)
  ├── UNIVERSE  (필수 ★)
  │     5개 심볼군 독립 실험, strict: |IR|>0.1 AND |t|>1.96
  ├── REGIME    (필수 ★)
  │     4축 × 3상태 (ETH가격/자기자신가격/ETHOI/자기자신OI × bull/bear/sideways)
  ├── TEMPORAL  (필수 ★)
  │     monthly rolling IR/t-stat, improving/decaying 추세 판단
  └── COMBO     (사실상 필수 ★★)
        feature/event 결합, 유의미한 결합 없으면 skip 허용
      │
      ▼
  INTERPRET (LLM)
  - 각 실험 → support | rebut + 근거
  - strict 미달 실험 자동 rebut 반영
  - abstract  생성 (핵심 발견 한줄)
  - discussion 생성 (한계, 노이즈, 추가 관찰)
  - POST /api/graph/node/{id}/experiments  → 실험 결과 저장
      │
      ▼
  JUDGE (LLM)
  - 전체 실험 + abstract 종합
  - accept | decline (binary)
  - PATCH /api/graph/node/{id}  → status, abstract_, discussion 업데이트
      │
      ▼
  [항상 amure-db 저장 — accept/decline 무관]
  - picked root node에만 엣지 추가
  - Step 2 판단 결과 재사용 (LLM 재호출 없음)
  ┌────────────────────────────────────────────────────┐
  │ reference / superset / subset picked:              │
  │   POST /api/graph/edge { kind, reason }            │
  │                                                    │
  │ orthogonal picked:                                 │
  │   POST /api/graph/edge { kind: orthogonal, reason }│
  │                                                    │
  │ 미pick 노드: 엣지 없음                             │
  └────────────────────────────────────────────────────┘
```

---

## API Reference

### Node

```
POST   /api/graph/node                    Hypothesis 생성 (embedding 자동)
GET    /api/graph/node/{id}               노드 조회
PATCH  /api/graph/node/{id}              status / abstract_ / discussion 업데이트
DELETE /api/graph/node/{id}              삭제 (엣지 cascade)

POST   /api/graph/node/{id}/experiments  실험 결과 추가
GET    /api/graph/node/{id}/experiments  실험 목록 조회
```

### Edge

```
POST   /api/graph/edge     엣지 추가 { source, target, kind, reason(필수) }
DELETE /api/graph/edge/{id}
```

### Search / RAG

```
GET /api/graph/search?q=&top_k=&status=
  status: accept | decline | draft | all
  embedding 기반 (fallback: keyword), Draft 항상 제외

GET /api/graph/search/balanced?q=&n=
  accept n개 + decline n개 균형 반환 (RAG 전용)

GET /api/graph/walk/{id}?hops=1&exclude_orthogonal=true
  1-hop 이웃 반환, orthogonal 엣지 제외 옵션
```

### Graph

```
GET  /api/graph/all
GET  /api/graph/summary
POST /api/graph/embed-all    전체 노드 임베딩 재생성
```

---

## Quick Start

```bash
# 1. .env 설정
echo "OPENAI_API_KEY=sk-..." > .env

# 2. 빌드 & 실행
cargo run  # :8081
```

## 환경변수

| 변수 | 설명 | 비고 |
|------|------|------|
| `OPENAI_API_KEY` | embedding 생성 | 없으면 keyword fallback |

---

## 마이그레이션 (v1 → v2)

```
1. POST /api/graph/embed-all  → 기존 노드 임베딩 생성
2. accepted Claim → Hypothesis(accept) 변환
3. Reason / Evidence / Experiment 노드 전부 삭제
4. 기존 엣지 전부 삭제
5. 새 엣지 LLM 기반 재구성
```

---

## Storage

```
data/amure_graph/
  ├── nodes.json    (embedding 포함, atomic write)
  └── edges.json
```
