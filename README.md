# amure-db

지식 그래프 기반 범용 RAG 데이터베이스.
Hypothesis 노드 + 의미론적 엣지 + OpenAI embedding 검색.

---

## Architecture

```
Any Client  →  amure-db (:8081)
                 - 노드 저장 (Hypothesis + Experiment 내장)
                 - OpenAI embedding 기반 RAG 검색
                 - 의미론적 그래프 관계 + BFS walk
                 - JSON atomic persistence
```

### Source Files

| File | Role |
|------|------|
| `node.rs` | Node (statement, abstract, discussion, status, experiments), Experiment |
| `edge.rs` | EdgeKind (Reference/Superset/Subset/Orthogonal), reason 필드 |
| `graph.rs` | in-memory 그래프, BFS walk, adjacency list |
| `search.rs` | embedding cosine → MMR reranking, balanced search |
| `embedding.rs` | OpenAI text-embedding-3-small (1536d) |
| `persistence.rs` | JSON atomic write (tmp-rename pattern) |
| `server.rs` | Axum HTTP 서버, CORS, dashboard |

---

## Core Concepts

### Node

모든 지식은 **단일 노드**로 표현된다.

```
Node {
    id          Uuid
    statement   String              // 핵심 주장 한 문장
    abstract_   String              // 요약 한줄평
    discussion  String              // 한계, 추가 관찰
    status      Draft | Accept | Decline
    experiments Vec<Experiment>     // 실험/검증 결과 내장
    embedding   Option<Vec<f32>>   // OpenAI text-embedding-3-small
}

Experiment {
    id      Uuid
    kind    String                  // 실험 종류 (자유 형식)
    target  String                  // 실험 대상/조건
    result  JSON                    // 결과 데이터
    verdict Support | Rebut
    note    Option<String>
}
```

### Edge

노드 간 의미론적 관계. **모든 엣지에 reason(사유) 필수.**

```
Edge {
    source  Uuid
    target  Uuid
    kind    Reference | Superset | Subset | Orthogonal
    reason  String    // 왜 이 관계인지
}
```

| 관계 | 의미 | 역방향 자동 생성 |
|------|------|-----------------|
| **reference** | 조건/메커니즘 교집합 | reference (대칭) |
| **superset** | source가 target을 포괄 | subset |
| **subset** | source가 target의 부분집합 | superset |
| **orthogonal** | 완전히 다른 영역 | orthogonal (대칭) |

**subset/superset edge 생성 시 역방향 edge가 자동으로 생성됨.**

---

## API Reference

### Node

```
POST   /api/graph/node                    노드 생성 (embedding 자동)
GET    /api/graph/node/{id}               노드 조회
PATCH  /api/graph/node/{id}               status / abstract_ / discussion 업데이트
DELETE /api/graph/node/{id}               삭제 (엣지 cascade)

POST   /api/graph/node/{id}/experiments   실험 결과 추가
GET    /api/graph/node/{id}/experiments   실험 목록 조회
```

### Edge

```
POST   /api/graph/edge     엣지 추가 { source, target, kind, reason }
                           subset/superset → 역방향 자동 생성
DELETE /api/graph/edge/{id}
```

### Search / RAG

```
GET /api/graph/search?q=&top_k=&status=
    status: accept | decline | draft | all
    embedding 기반 (3-layer: cosine → graph walk → MMR reranking)
    Draft 항상 제외

GET /api/graph/search/balanced?q=&n=
    accept n개 + decline n개 균형 반환

GET /api/graph/walk/{id}?hops=1&exclude_orthogonal=true
    BFS walk, orthogonal 제외 옵션

GET /api/graph/similar/{id}?top_k=5
    embedding 유사 노드

GET /api/graph/subgraph/{id}
    시각화용 서브그래프 추출
```

### Graph

```
GET  /api/graph/all
GET  /api/graph/summary
POST /api/graph/embed-all    전체 노드 임베딩 재생성
POST /api/save               명시적 디스크 저장
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
| `OPENAI_API_KEY` | embedding 생성 | 없으면 검색 비활성 |

---

## Storage

```
data/amure_graph/
  ├── nodes.json    (embedding 포함, atomic write)
  └── edges.json
```
