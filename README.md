# amure-db

Graph-based knowledge database with hybrid RAG search. Runs as a standalone HTTP server on port 8081.

## Architecture

```
amure-db (:8081)
  ├── AmureGraph (in-memory adjacency list, HashMap<Uuid, Node/Edge>)
  ├── Hybrid RAG Search (embedding → graph walk → MMR, fallback to token+synonym)
  ├── SynonymDict (30+ Korean/English quant term groups)
  ├── Yahoo Finance → Fact nodes
  ├── LLM integration (auto-tag, summarize, verify)
  ├── Knowledge analysis (failure warning, contradiction detection, gap claims)
  ├── Graph intelligence (causal chains, impact analysis, temporal health)
  └── Dashboard (force-directed graph, port 8081)
```

### Source Files

| File | Role |
|------|------|
| `node.rs` | Node types, Status, tokenizer, embedding field |
| `edge.rs` | Edge types, weight, note |
| `graph.rs` | In-memory graph engine — BFS walk, causal chains, verdict propagation, dependency detection |
| `search.rs` | Hybrid RAG — embedding cosine → graph walk → MMR reranking (keyword fallback) |
| `embedding.rs` | OpenAI `text-embedding-3-small` — single/batch, cosine similarity |
| `synonym.rs` | Korean/English quant synonym dictionary |
| `persistence.rs` | JSON atomic write (tmp → rename) |
| `server.rs` | Axum HTTP server, all API endpoints |

## Quick Start

```bash
cargo build --release

# With semantic search (optional)
export OPENAI_API_KEY=sk-...

./target/release/amure-server
# → http://localhost:8081
```

## Core Concepts

### Nodes

| Kind | Description |
|------|-------------|
| **Claim** | 검증 가능한 명제. Knowledge graph의 핵심 단위 |
| **Reason** | Claim을 지지(Support) 또는 반박(Rebut)하는 논리 |
| **Evidence** | Reason을 뒷받침하는 구체적 근거 |
| **Experiment** | Evidence를 생산하는 실험 |
| **Fact** | 외부 데이터 (Yahoo Finance 등) |

Status: `Draft` → `Active` → `Accepted` / `Rejected` / `Weakened`

### Edges

| Kind | Direction | Description |
|------|-----------|-------------|
| **Support** | Reason → Claim | 지지 |
| **Rebut** | Reason → Claim | 반박 |
| **DependsOn** | A → B | A가 참이려면 B가 참이어야 함 |
| **Contradicts** | A ↔ B | 동시 참 불가 |
| **Refines** | A → B | A는 B의 구체적 버전 |
| **DerivedFrom** | A → B | A는 B에서 파생 |

### Hybrid RAG Search

```
Query: "미결제약정 추세"
  ↓
Step 1: OpenAI embedding → cosine similarity → top entry points
        (OPENAI_API_KEY 없으면 token match + synonym expansion으로 fallback)
  ↓
Step 2: Graph walk (1-2 hop BFS from entry points)
        entry_score × 0.5^hop → distance-decayed candidates
  ↓
Step 3: MMR reranking (λ=0.7)
        Jaccard similarity on keywords → relevance vs. diversity balance
  ↓
Output: ranked results, failed paths labeled "이 경로는 이미 실패했다"
```

- 노드 생성 시 embedding 비동기 자동 계산 (응답 블로킹 없음)
- `OPENAI_API_KEY` 없어도 keyword+synonym 검색으로 graceful degradation

## API Reference

### Graph CRUD

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/graph/all` | 전체 노드 + 엣지 |
| GET | `/api/graph/summary` | 통계 (노드/엣지 수, 종류별) |
| GET | `/api/graph/search?q=&top_k=10&include_failed=` | Hybrid RAG 검색 |
| GET | `/api/graph/node/{id}` | 노드 상세 + 연결된 엣지 |
| POST | `/api/graph/node` | 노드 추가 → embedding 비동기 계산 |
| PATCH | `/api/graph/node/{id}` | 노드 업데이트 |
| DELETE | `/api/graph/node/{id}` | 노드 삭제 (엣지 cascade) |
| POST | `/api/graph/edge` | 엣지 추가 |
| DELETE | `/api/graph/edge/{id}` | 엣지 삭제 |
| GET | `/api/graph/walk/{id}?hops=2` | BFS 탐색 |
| GET | `/api/graph/subgraph/{id}` | 서브그래프 추출 (시각화용) |

### Embedding

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/graph/similar/{id}?top_k=5` | 임베딩 기반 유사 노드 |
| GET | `/api/graph/unrelated/{id}?top_k=5` | 임베딩 기반 비유사 노드 |
| POST | `/api/graph/embed-all` | embedding 없는 모든 노드 일괄 생성 |

### Graph Intelligence

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/graph/causal-chains/{id}` | 인과 체인 탐색 |
| GET | `/api/graph/temporal-health` | 시간별 유효성 + 재검증 스케줄 |
| GET | `/api/graph/impact/{id}` | 기각 시 영향 분석 (역방향 전파) |
| GET | `/api/graph/dependencies/{id}` | 의존성 트리 |

### Edge Propagation

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/graph/propagate-verdict/{id}` | Experiment verdict → Reason/Claim 상태 자동 전파 |
| POST | `/api/graph/detect-dependencies/{id}` | Claim 간 Jaccard → DependsOn 엣지 자동생성 |
| POST | `/api/graph/on-accept/{id}` | Accept 시 Support/Refines/Contradicts 자동 연결 |

### Knowledge Analysis

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/check-failures` | 유사 실패 경고 + 미사용 방법론 표시 |
| GET | `/api/check-revalidation` | 30일+ Knowledge 재검증 알림 |
| POST | `/api/detect-contradictions` | 충돌 감지 + Contradicts 엣지 생성 |
| POST | `/api/auto-gap-claims` | Verdict gaps → Draft Claim 자동 생성 |
| GET | `/api/suggest-combinations` | 실패 실험 결합 아이디어 제안 |

### Yahoo Finance

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/yahoo/fetch` | 종목 1개 → Fact 노드 생성 |
| POST | `/api/yahoo/batch` | 여러 종목 일괄 생성 |
| POST | `/api/yahoo/auto-organize` | Fact → Claim 자동 그룹핑 |

### LLM

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/llm/auto-tag` | 노드 키워드 자동 생성 |
| POST | `/api/llm/auto-tag-all` | 전체 Fact 일괄 태깅 |
| POST | `/api/llm/summarize` | RAG 결과 요약 |
| POST | `/api/llm/explain-groups` | 그룹 경제적 이유 설명 |
| POST | `/api/llm/verify-claim` | Claim 논리적 타당성 평가 |

## Storage

```
data/amure_graph/
  ├── nodes.json    (embedding 벡터 포함, atomic write)
  └── edges.json
```

## Integration

AlphaFactor (`:8080`) → HTTP → amure-db (`:8081`)

amure-db가 먼저 기동되어야 함. AlphaFactor는 amure-db 다운 시 빈 결과로 graceful degradation.

## Design Notes

- **Hybrid search**: embedding 있으면 semantic search, 없으면 token+synonym — 항상 동작
- **Explainable**: edge를 따라가면 왜 이 결과가 나왔는지 추적 가능
- **Failure is knowledge**: rejected/weakened 노드도 검색 포함, 실패 경로 라벨링
- **Auto propagation**: verdict → reason → claim 상태 자동 전파
