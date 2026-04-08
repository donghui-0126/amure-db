# amure-db v2 Redesign

## 핵심 변경 방향

기존 Claim/Reason/Evidence/Experiment 4계층 → **Hypothesis 단일 노드**로 단순화.
기존 Support/Rebut/DependsOn/Contradicts 엣지 → **reference/superset/subset/orthogonal**로 교체.

---

## 1. Node 구조 변경

### 기존
```
NodeKind: Claim | Reason | Evidence | Experiment
NodeStatus: Draft | Active | Accepted | Rejected | Weakened | NeedsResearch
fields: statement, keywords, embedding
```

### v2
```rust
NodeKind: Hypothesis  // 단일 종류

NodeStatus: Draft | Accept | Decline

Node {
    id: Uuid
    statement: String           // 가설 한 문장
    abstract_: String           // 핵심 발견 한줄평 (interpret 후 채워짐)
    discussion: String          // 한계, 노이즈, 추가 관찰 (interpret 후 채워짐)
    status: NodeStatus          // Draft → Accept | Decline
    experiments: Vec<Experiment> // 실험 결과 내장
    embedding: Option<Vec<f32>> // OpenAI text-embedding-3-small
    created_at, updated_at
}

Experiment {
    id: Uuid
    kind: ExperimentKind        // Universe | Regime | Temporal | Combo
    target: String              // 심볼군 / regime 축 / 기간 등
    result: serde_json::Value   // Julia 결과 (IR, t-stat, hit_ratio 등)
    verdict: Verdict            // Support | Rebut
    note: Option<String>        // 해석 메모
}

ExperimentKind: Universe | Regime | Temporal | Combo

Verdict: Support | Rebut
```

---

## 2. Edge 구조 변경

### 기존
```
EdgeKind: Support | Rebut | DependsOn | Contradicts
reason 필드 없음
```

### v2
```rust
EdgeKind: Reference | Superset | Subset | Orthogonal

Edge {
    id: Uuid
    source: Uuid
    target: Uuid
    kind: EdgeKind
    reason: String    // 필수 — 왜 이 관계인지 사유
    created_at
}
```

**관계 의미:**
- `reference`  — 교집합 있음 (조건/메커니즘 공유)
- `superset`   — source가 target을 포함하는 상위개념
- `subset`     — source가 target의 부분집합
- `orthogonal` — 완전히 다른 영역, 겹침 없음

---

## 3. API 변경

### 제거
```
POST /api/claim                          ← Claim 전용, 불필요
POST /api/graph/propagate-verdict/{id}   ← Reason 전파 로직
POST /api/graph/on-accept/{id}           ← 승인 시 자동 엣지 (v2에선 AlphaFactor가 직접)
POST /api/graph/detect-dependencies/{id} ← 키워드 기반 자동 링크
POST /api/check-failures                 ← 실패 패턴 감지
POST /api/detect-contradictions          ← 모순 감지
GET  /api/graph/temporal-health          ← 재검증 스케줄
```

### 유지 (수정 포함)
```
POST /api/graph/node
  - body: { statement, status?, abstract_?, discussion? }
  - 생성 시 embedding 자동 생성 (OPENAI_API_KEY 있으면)

PATCH /api/graph/node/{id}
  - status, abstract_, discussion, experiments 업데이트 가능

DELETE /api/graph/node/{id}

POST /api/graph/edge
  - body: { source, target, kind, reason }  ← reason 필수 추가

DELETE /api/graph/edge/{id}

GET /api/graph/search?q=&top_k=&status=&include_decline=
  - status 필터: accept | decline | draft | all
  - include_decline: true면 decline도 포함 (RAG용)
  - embedding 기반 검색 (fallback: keyword)

GET /api/graph/walk/{id}?hops=1
  - 1-hop 이웃 반환
  - orthogonal 엣지 제외 옵션 추가: ?exclude_orthogonal=true

GET /api/graph/node/{id}
GET /api/graph/all
GET /api/graph/summary
```

### 신규
```
POST /api/graph/node/{id}/experiments
  - 실험 결과 추가/업데이트
  - body: { kind, target, result, verdict, note? }

GET /api/graph/node/{id}/experiments
  - 실험 목록 조회

POST /api/graph/embed-all
  - 전체 노드 임베딩 재생성 (마이그레이션용)

GET /api/graph/search/balanced?q=&n=
  - accept n개 + decline n개 균형 반환 (RAG용)
```

---

## 4. Search / RAG 변경

### 기존
```rust
// OPENAI_API_KEY 없으면 keyword fallback
// include_failed 플래그로 Rejected/Weakened 포함 여부 결정
// Draft 노드도 결과에 포함됨
```

### v2
```rust
// Draft 노드는 RAG 결과에서 항상 제외
// accept/decline 균형 샘플링 지원
// orthogonal 엣지 walk 제외 옵션
// 1-hop context는 root node의 기존 엣지 그대로 반환
```

---

## 5. dotenv 수정

```toml
# Cargo.toml
[dependencies]
dotenvy = "0.15"
```

```rust
// main.rs
fn main() {
    dotenvy::dotenv().ok();  // .env 자동 로드
    // ...
}
```

---

## 6. 마이그레이션

```
기존 accepted Claim → Hypothesis(accept)로 변환
  - statement 유지
  - abstract_ = "" (빈값, 나중에 LLM 재생성 가능)
  - experiments = [] (빈값)
  - embedding 재생성 필요

기존 Reason, Evidence, Experiment 노드 → 전부 삭제
기존 Support/Rebut/DependsOn/Contradicts 엣지 → 전부 삭제
기존 declined/rejected Claim → Hypothesis(decline)로 변환 (선택)

마이그레이션 순서:
1. embed-all 실행으로 기존 노드 임베딩 생성
2. accepted Claim → Hypothesis(accept) 변환
3. 나머지 전부 삭제
4. 새 엣지 수동 or LLM으로 재구성
```

---

## 7. AlphaFactor (engine) 연동 변경

```
GENERATE 단계:
  POST /api/graph/node  ← Hypothesis 생성 (draft)

EXECUTE/INTERPRET 단계:
  POST /api/graph/node/{id}/experiments  ← 실험별 결과 추가

JUDGE 단계:
  PATCH /api/graph/node/{id}  ← status(accept|decline), abstract_, discussion 업데이트

엣지 추가 (picked root node에만):
  POST /api/graph/edge  ← { source: new_id, target: picked_id, kind, reason }

RAG:
  GET /api/graph/search/balanced?q=idea&n=5
  GET /api/graph/walk/{id}?hops=1&exclude_orthogonal=true
```

---

## 8. 구현 순서

1. `dotenvy` 추가 → OPENAI_API_KEY 로드 버그 수정
2. `Node` / `Experiment` / `Edge` 구조체 변경
3. `NodeStatus` / `EdgeKind` enum 변경
4. API 엔드포인트 정리 (제거/유지/신규)
5. Search: balanced RAG, orthogonal 제외 walk
6. 마이그레이션 스크립트
7. embed-all 재실행
