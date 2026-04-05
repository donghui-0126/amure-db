# amure-db

Graph-based knowledge database with RAG search. No embeddings, no external models.

## Architecture

```
amure-db (:8081) — standalone API server
  ├── AmureGraph (in-memory adjacency list)
  ├── 3-layer RAG (token match → graph walk → MMR)
  ├── SynonymDict (30+ Korean/English quant term groups)
  ├── Yahoo Finance → Fact nodes
  ├── LLM integration (auto-tag, summarize, verify, explain)
  ├── Knowledge utilization (failure warning, revalidation, contradictions)
  ├── Graph Intelligence (causal chains, temporal health, impact, dependencies)
  ├── Edge Propagation (verdict→reason, claim dependencies, accept relations)
  └── Dashboard (force-directed graph visualization)
```

### Source Files

| File | Role |
|------|------|
| `node.rs` | Node 타입(Claim/Reason/Evidence/Experiment/Fact), Status, 한/영 토크나이저 |
| `edge.rs` | Edge 타입(Support/Rebut/DependsOn/Contradicts/Refines/DerivedFrom), weight, note |
| `graph.rs` | 핵심 엔진 — HashMap 인메모리 그래프, adjacency list, BFS walk, 인과체인, temporal health, verdict 전파, 의존성 자동감지, accept 시 관계 자동생성 |
| `search.rs` | 3-Layer Graph RAG — Token Match + 동의어 확장 → BFS Graph Walk → MMR Reranking |
| `synonym.rs` | 한/영 퀀트 용어 동의어 사전 (30+ 그룹) |
| `persistence.rs` | JSON atomic write (tmp → rename), nodes.json + edges.json |
| `server.rs` | Axum HTTP 서버 (port 8081), 모든 API 엔드포인트 |
| `lib.rs` | 모듈 re-export |

## Quick Start

```bash
cd amure-db
cargo build --release
./target/release/amure-server
# → http://localhost:8081
```

Dashboard: http://localhost:8081

## Core Concepts

### Nodes

| Kind | Description |
|------|-------------|
| **Claim** | 검증 가능한 명제. Knowledge graph의 핵심 단위 |
| **Reason** | Claim을 지지(Support) 또는 반박(Rebut)하는 논리 |
| **Evidence** | Reason을 뒷받침하는 구체적 근거 |
| **Experiment** | Evidence를 생산하는 실험 |
| **Fact** | 외부 데이터 (Yahoo Finance 등) |

Status: `Draft` → `Active` → `Accepted` (Knowledge) / `Rejected` / `Weakened`

### Edges

| Kind | Description |
|------|-------------|
| **Support** | A가 B를 지지 |
| **Rebut** | A가 B를 반박 |
| **DependsOn** | A는 B에 의존 |
| **Contradicts** | A와 B는 충돌 |
| **Refines** | A는 B의 구체적 버전 |
| **DerivedFrom** | A는 B에서 파생 |

### 3-Layer RAG Search

```
Query: "미결제약정 추세"
  ↓
Layer 1: Token match + synonym expansion
  "미결제약정" → ["oi", "open_interest", "미결제약정"]
  → Entry points: OI-related nodes
  ↓
Layer 2: Graph walk (1-2 hop BFS)
  OI Reason → connected Claim → connected Experiment
  → Candidate expansion with distance-decayed scores
  ↓
Layer 3: MMR reranking (λ=0.7)
  Jaccard similarity on keywords → diverse final results
  → Failed nodes labeled: "이 경로는 이미 실패했다"
```

No embeddings. No neural models. Token matching + synonym dictionary + graph structure.

## API Reference

### Graph CRUD

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/graph/all` | 전체 노드 + 엣지 |
| GET | `/api/graph/summary` | 통계 (노드/엣지 수, 종류별) |
| GET | `/api/graph/search?q=&top_k=10&include_failed=` | RAG 검색 |
| GET | `/api/graph/node/{id}` | 노드 상세 + 연결된 엣지 |
| POST | `/api/graph/node` | 노드 추가 (kind, statement, keywords, metadata, status) |
| PATCH | `/api/graph/node/{id}` | 노드 업데이트 (status, metadata, keywords, statement) |
| DELETE | `/api/graph/node/{id}` | 노드 삭제 (엣지 cascade) |
| POST | `/api/graph/edge` | 엣지 추가 (source, target, kind, note) |
| DELETE | `/api/graph/edge/{id}` | 엣지 삭제 |
| GET | `/api/graph/walk/{id}?hops=2` | BFS 탐색 |
| GET | `/api/graph/subgraph/{id}` | 서브그래프 추출 (10 hop, 시각화용) |

### Graph Intelligence

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/graph/causal-chains/{id}` | 인과 체인 탐색 (Support/DependsOn/DerivedFrom/Refines 따라감) |
| GET | `/api/graph/temporal-health` | 시간별 유효성 tracking + 재검증 스케줄 |
| GET | `/api/graph/impact/{id}` | 기각 시 영향 분석 (역방향 전파) |
| GET | `/api/graph/dependencies/{id}` | 의존성 트리 (DependsOn/Support 체인) |

### Edge Propagation

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/graph/propagate-verdict/{id}` | Experiment verdict → Reason/Claim 상태 자동 전파 |
| POST | `/api/graph/detect-dependencies/{id}` | Claim 간 키워드 Jaccard → DependsOn 엣지 자동생성 |
| POST | `/api/graph/on-accept/{id}` | Accept 시 기존 Claim과 Support/Refines/Contradicts 자동 연결 |

### Knowledge Utilization

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/check-failures` | 유사 실패 경고 + 이전 실험/미사용 방법론 표시 |
| GET | `/api/check-revalidation` | 30일+ Knowledge 재검증 알림 |
| POST | `/api/detect-contradictions` | Knowledge 간 충돌 감지 + Contradicts 엣지 생성 |
| POST | `/api/auto-gap-claims` | Verdict gaps → 새 Draft Claim 자동 생성 |
| GET | `/api/suggest-combinations` | 실패 실험 결합 → 새 아이디어 제안 |

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
| POST | `/api/llm/summarize` | RAG 결과 한 문단 요약 |
| POST | `/api/llm/explain-groups` | 그룹 경제적 이유 설명 |
| POST | `/api/llm/verify-claim` | Claim 논리적 타당성 평가 |

### Legacy / Utility

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/claim` | 레거시 Claim 생성 |
| POST | `/api/edge` | 레거시 Edge 생성 |
| POST | `/api/save` | 수동 디스크 저장 |

## Synonym Dictionary

30+ 한국어/영어 퀀트 용어 그룹:

```
OI ↔ open_interest ↔ 미결제약정
momentum ↔ 모멘텀 ↔ 추세
volatility ↔ 변동성 ↔ vol_regime
funding ↔ funding_rate ↔ 펀딩 ↔ 펀딩레이트
cross_sectional ↔ 횡단면 ↔ CS
bull ↔ 상승장 ↔ 강세
bear ↔ 하락장 ↔ 약세
...
```

## Knowledge Utilization Flow

```
새 가설 입력 → [실패 경고]
  "OI momentum을 해보고 싶어"
  → ⚠️ "전에 3개 실험(CS, Distributional, Regime)으로 시도했는데 다 미유의"
  → "Conditional, DoseResponse, Temporal은 안 해봤어 ← 여기 기회"

실패 결합 → [새 아이디어 제안]
  Volume spike(방향 없음) + Funding extreme(decay)
  → "동시 발생 시 mean reversion 증폭 가능?"
  → 실험 결과: Vol>3x + prem_z>2 → SHORT +15.2bp (t=6.3) ✓

Knowledge 충돌 → [자동 감지]
  "reversal이 지배적" ↔ "BTC lead는 momentum"
  → Contradicts 엣지 자동 생성

30일 경과 → [재검증 알림]
  "이 시그널 아직 유효한지 확인하세요"
```

## Test Results

### Unit Tests: 23 passed

```bash
cd amure-db && cargo test
```

### RAG Validation: 14/15 (93%)

Yahoo Finance 15종목 기반 검색 품질:
- 티커 검색 (AAPL, NVDA): 100%
- 섹터 검색 (tech ai): ✓
- 한국어 (배당 가치주): ✓
- 동의어 (미결제약정 → OI): ✓
- Graph walk (conviction → Claim): ✓
- MMR diversity: ✓
- Failed path labeling: ✓

## Storage

```
data/amure_graph/
  ├── nodes.json    (atomic write via .tmp → rename)
  └── edges.json
```

JSON file-based. No external database. Atomic writes prevent corruption.

## Integration with AlphaFactor

AlphaFactor (:8080) calls amure-db (:8081) via HTTP:

```
AlphaFactor (AmureClient)
  → reqwest::Client
  → http://localhost:8081/api/...
  → amure-db (sole data owner)
```

amure-db must start before AlphaFactor. Graceful degradation: if amure-db is down, AlphaFactor returns empty results.

## Design Philosophy

- **No embeddings**: token matching + synonym dictionary + graph structure
- **Explainable**: "왜 이 결과가 나왔는지" edge를 따라가면 보임
- **Failure is knowledge**: rejected/weakened 노드도 검색에 포함, 실패 경로 라벨링
- **Graph > Vector**: 관계(support/rebut/contradicts)가 유사도보다 정보량이 많음
- **Korean + English**: 동의어 사전으로 양언어 지원
- **Auto propagation**: verdict → reason → claim 상태 자동 전파, 관계 자동 감지
- **Atomic persistence**: JSON tmp → rename 패턴으로 데이터 무결성 보장
