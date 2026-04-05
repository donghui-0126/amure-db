# amure-db — Graph RAG Knowledge Database

## Overview
독립 그래프 데이터베이스. HTTP API (port 8081)로 서비스.
AlphaFactor 등 외부 시스템이 HTTP로 호출하는 구조.
임베딩 없이 토큰+동의어+그래프워크+MMR로 검색.

## Architecture
```
AmureGraph (in-memory)
  ├── nodes: HashMap<Uuid, Node>      — adjacency list 기반
  ├── edges: HashMap<Uuid, Edge>
  ├── adjacency / reverse_adj         — outgoing/incoming edge index
  └── persistence: JSON (data/amure_graph/)
```

## Node Types (node.rs)
| Kind | 용도 | 초기 Status |
|------|------|------------|
| Claim | 검증 대상 가설 ("OI는 momentum 선행지표") | Draft |
| Reason | Claim을 지지/반박하는 논거 | Active |
| Evidence | 실험 결과에서 나온 증거 | Active |
| Experiment | 설계된 실험 (method, config, if_true/if_false) | Draft |
| Fact | 외부 데이터 (Yahoo Finance 등) | Active |

## Node Status
Draft → Active → Accepted / Rejected / Weakened

## Edge Types (edge.rs)
| Kind | 방향 | 의미 |
|------|------|------|
| Support | Reason → Claim | Claim 지지 근거 |
| Rebut | Reason → Claim | Claim 반박 반론 |
| DependsOn | A → B | A가 참이려면 B가 먼저 참이어야 함 |
| Contradicts | A ↔ B | A와 B 동시 참 불가 |
| Refines | A → B | A는 B의 더 구체적 버전 |
| DerivedFrom | A → B | A는 B 실험/분석에서 파생 |

Edge fields: id, source, target, kind, weight (f64), note (String), created_at

## 실험 파이프라인 엣지 흐름
```
Claim 생성        → (엣지 없음)
Reason 추가       → Support/Rebut: Reason → Claim
Evidence 추가     → DerivedFrom: Evidence → Reason
Experiment 추가   → DependsOn: Experiment → Reason
Verdict 결과      → Evidence 생성 + DerivedFrom: Evidence → Reason
Gap 하위 Claim    → Refines: GapClaim → 원본Claim
Fact 연결         → DerivedFrom: Fact → Claim
충돌 감지         → Contradicts: Claim ↔ Claim
```

## Search (search.rs) — 3-Layer Graph RAG
1. **Token Match + Synonym**: 쿼리 토큰화 → 동의어 확장 → 노드 매칭
2. **Graph Walk**: 매칭된 노드에서 1-2 hop BFS → 관련 노드 확장
3. **MMR Reranking**: relevance × diversity 균형 (lambda=0.7)

## API Endpoints (server.rs, port 8081)
```
# Core CRUD
GET  /api/graph/all                  — 전체 노드+엣지
GET  /api/graph/summary              — 노드/엣지 수 통계
GET  /api/graph/search?q=&top_k=     — RAG 검색
GET  /api/graph/node/{id}            — 노드 + 연결 엣지
POST /api/graph/node                 — 노드 생성
PATCH /api/graph/node/{id}           — 노드 업데이트
DELETE /api/graph/node/{id}          — 노드 삭제
POST /api/graph/edge                 — 엣지 생성
DELETE /api/graph/edge/{id}          — 엣지 삭제
GET  /api/graph/walk/{id}            — BFS walk
GET  /api/graph/subgraph/{id}        — 서브그래프 추출

# Knowledge Analysis
POST /api/check-failures             — 유사 실패 패턴 경고
GET  /api/check-revalidation         — 재검증 필요 Knowledge
POST /api/detect-contradictions      — 충돌 자동 감지
POST /api/auto-gap-claims            — Gap → 하위 Claim 자동 생성
GET  /api/suggest-combinations       — 실패 실험 결합 제안

# Graph Intelligence
GET  /api/graph/causal-chains/{id}   — 인과 체인 탐색
GET  /api/graph/temporal-health      — 시간별 유효성 tracking
GET  /api/graph/impact/{id}          — 기각 시 영향 분석
GET  /api/graph/dependencies/{id}    — 의존성 트리

# Yahoo Finance
POST /api/yahoo/fetch                — 종목 데이터 fetch
POST /api/yahoo/batch                — 배치 fetch
POST /api/yahoo/auto-organize        — 자동 Fact 정리

# LLM
POST /api/llm/auto-tag               — 키워드 자동 태깅
POST /api/llm/summarize              — 검색 결과 요약
POST /api/llm/verify-claim           — Claim 검증
```

## Build & Run
```bash
cargo build --release
./target/release/amure-server        # port 8081
```

## Coding Rules
- edition = "2024"
- Pattern matching: no `&count` in closures
- Persistence: JSON atomic write (write tmp → rename)
- 동의어: synonym.rs에 30+ 한/영 그룹
- 토크나이저: 한/영 혼합 지원, 2자 이상만
