/// AmureGraph — 인메모리 그래프 엔진.
/// Adjacency list 기반, BFS walk, 노드/엣지 CRUD.

use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

use crate::edge::{Edge, EdgeKind};
use crate::node::{Node, NodeKind, NodeStatus};

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Out,
    In,
    Both,
}

pub struct AmureGraph {
    pub nodes: HashMap<Uuid, Node>,
    pub edges: HashMap<Uuid, Edge>,
    adjacency: HashMap<Uuid, Vec<Uuid>>,    // node_id → outgoing edge_ids
    reverse_adj: HashMap<Uuid, Vec<Uuid>>,  // node_id → incoming edge_ids
}

impl AmureGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            adjacency: HashMap::new(),
            reverse_adj: HashMap::new(),
        }
    }

    // ── Node CRUD ──────────────────────────────────────────────────────

    pub fn add_node(&mut self, node: Node) -> Uuid {
        let id = node.id;
        self.nodes.insert(id, node);
        self.adjacency.entry(id).or_default();
        self.reverse_adj.entry(id).or_default();
        id
    }

    pub fn get_node(&self, id: &Uuid) -> Option<&Node> {
        self.nodes.get(id)
    }

    pub fn get_node_mut(&mut self, id: &Uuid) -> Option<&mut Node> {
        self.nodes.get_mut(id)
    }

    pub fn remove_node(&mut self, id: &Uuid) -> Option<Node> {
        // Collect edge IDs to remove
        let mut edge_ids = Vec::new();
        if let Some(out_edges) = self.adjacency.get(id) {
            edge_ids.extend(out_edges.iter());
        }
        if let Some(in_edges) = self.reverse_adj.get(id) {
            edge_ids.extend(in_edges.iter());
        }
        let edge_ids: Vec<Uuid> = edge_ids.into_iter().copied().collect();

        // Remove edges
        for eid in edge_ids {
            self.remove_edge(&eid);
        }

        self.adjacency.remove(id);
        self.reverse_adj.remove(id);
        self.nodes.remove(id)
    }

    pub fn nodes_by_kind(&self, kind: NodeKind) -> Vec<&Node> {
        self.nodes.values().filter(|n| n.kind == kind).collect()
    }

    pub fn nodes_by_status(&self, status: NodeStatus) -> Vec<&Node> {
        self.nodes.values().filter(|n| n.status == status).collect()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    // ── Edge CRUD ──────────────────────────────────────────────────────

    pub fn add_edge(&mut self, edge: Edge) -> Uuid {
        let id = edge.id;
        self.adjacency.entry(edge.source).or_default().push(id);
        self.reverse_adj.entry(edge.target).or_default().push(id);
        self.edges.insert(id, edge);
        id
    }

    pub fn get_edge(&self, id: &Uuid) -> Option<&Edge> {
        self.edges.get(id)
    }

    pub fn remove_edge(&mut self, id: &Uuid) -> Option<Edge> {
        if let Some(edge) = self.edges.remove(id) {
            if let Some(adj) = self.adjacency.get_mut(&edge.source) {
                adj.retain(|eid| eid != id);
            }
            if let Some(radj) = self.reverse_adj.get_mut(&edge.target) {
                radj.retain(|eid| eid != id);
            }
            Some(edge)
        } else {
            None
        }
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    // ── Traversal ──────────────────────────────────────────────────────

    /// 노드의 이웃 조회. direction + edge_kind 필터.
    pub fn neighbors(
        &self,
        node_id: &Uuid,
        direction: Direction,
        edge_filter: Option<&[EdgeKind]>,
    ) -> Vec<(Uuid, &Edge)> {
        let mut result = Vec::new();

        let check_filter = |edge: &Edge| -> bool {
            edge_filter.map_or(true, |kinds| kinds.contains(&edge.kind))
        };

        // Outgoing
        if matches!(direction, Direction::Out | Direction::Both) {
            if let Some(edge_ids) = self.adjacency.get(node_id) {
                for eid in edge_ids {
                    if let Some(edge) = self.edges.get(eid) {
                        if check_filter(edge) {
                            result.push((edge.target, edge));
                        }
                    }
                }
            }
        }

        // Incoming
        if matches!(direction, Direction::In | Direction::Both) {
            if let Some(edge_ids) = self.reverse_adj.get(node_id) {
                for eid in edge_ids {
                    if let Some(edge) = self.edges.get(eid) {
                        if check_filter(edge) {
                            result.push((edge.source, edge));
                        }
                    }
                }
            }
        }

        result
    }

    /// BFS walk. max_hops 이내 도달 가능한 노드 + 거리 반환.
    pub fn walk(
        &self,
        start: &Uuid,
        max_hops: usize,
        edge_filter: Option<&[EdgeKind]>,
    ) -> Vec<(Uuid, usize)> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        visited.insert(*start);
        queue.push_back((*start, 0usize));

        while let Some((node_id, depth)) = queue.pop_front() {
            result.push((node_id, depth));

            if depth >= max_hops {
                continue;
            }

            for (neighbor_id, _edge) in self.neighbors(&node_id, Direction::Both, edge_filter) {
                if visited.insert(neighbor_id) {
                    queue.push_back((neighbor_id, depth + 1));
                }
            }
        }

        result
    }

    /// 노드 ID 목록에서 서브그래프 추출 (시각화용)
    pub fn subgraph(&self, node_ids: &[Uuid]) -> (Vec<&Node>, Vec<&Edge>) {
        let id_set: HashSet<&Uuid> = node_ids.iter().collect();
        let nodes: Vec<&Node> = node_ids.iter().filter_map(|id| self.nodes.get(id)).collect();
        let edges: Vec<&Edge> = self.edges.values()
            .filter(|e| id_set.contains(&e.source) && id_set.contains(&e.target))
            .collect();
        (nodes, edges)
    }

    /// 통계 요약
    pub fn summary(&self) -> GraphSummary {
        let mut kind_counts = HashMap::new();
        for node in self.nodes.values() {
            *kind_counts.entry(format!("{:?}", node.kind)).or_insert(0usize) += 1;
        }
        let mut edge_counts = HashMap::new();
        for edge in self.edges.values() {
            *edge_counts.entry(format!("{:?}", edge.kind)).or_insert(0usize) += 1;
        }
        let failed = self.nodes.values().filter(|n| n.is_failed()).count();

        GraphSummary {
            n_nodes: self.nodes.len(),
            n_edges: self.edges.len(),
            n_failed: failed,
            node_kinds: kind_counts,
            edge_kinds: edge_counts,
        }
    }

    // ── Causal Chain Inference ─────────────────────────────────────────

    /// A→B→C 인과 체인 탐색. directed edge만 따라감.
    /// Support/DependsOn/DerivedFrom/Refines를 "인과" 방향으로 봄.
    pub fn causal_chains(
        &self,
        start: &Uuid,
        max_depth: usize,
    ) -> Vec<CausalChain> {
        let causal_edges = [EdgeKind::Support, EdgeKind::DependsOn, EdgeKind::DerivedFrom, EdgeKind::Refines];
        let mut chains = Vec::new();
        let mut stack: Vec<(Uuid, Vec<Uuid>, Vec<EdgeKind>)> = vec![(*start, vec![*start], vec![])];

        while let Some((current, path, edge_types)) = stack.pop() {
            if path.len() > max_depth + 1 { continue; }

            let mut has_child = false;
            // Follow outgoing causal edges
            if let Some(edge_ids) = self.adjacency.get(&current) {
                for eid in edge_ids {
                    if let Some(edge) = self.edges.get(eid) {
                        if causal_edges.contains(&edge.kind) && !path.contains(&edge.target) {
                            has_child = true;
                            let mut new_path = path.clone();
                            new_path.push(edge.target);
                            let mut new_types = edge_types.clone();
                            new_types.push(edge.kind);
                            stack.push((edge.target, new_path, new_types));
                        }
                    }
                }
            }
            // Also follow incoming causal edges (reverse direction)
            if let Some(edge_ids) = self.reverse_adj.get(&current) {
                for eid in edge_ids {
                    if let Some(edge) = self.edges.get(eid) {
                        if causal_edges.contains(&edge.kind) && !path.contains(&edge.source) {
                            has_child = true;
                            let mut new_path = path.clone();
                            new_path.push(edge.source);
                            let mut new_types = edge_types.clone();
                            new_types.push(edge.kind);
                            stack.push((edge.source, new_path, new_types));
                        }
                    }
                }
            }

            // Terminal node or max depth — save chain if length > 1
            if (!has_child || path.len() > max_depth) && path.len() > 1 {
                let nodes: Vec<ChainNode> = path.iter().filter_map(|id| {
                    self.nodes.get(id).map(|n| ChainNode {
                        id: *id,
                        kind: format!("{:?}", n.kind),
                        statement: n.statement.clone(),
                        status: format!("{:?}", n.status),
                    })
                }).collect();

                chains.push(CausalChain {
                    nodes,
                    edge_types: edge_types.iter().map(|e| format!("{:?}", e)).collect(),
                    length: path.len(),
                });
            }
        }

        // Sort by length (longest chains first), deduplicate
        chains.sort_by(|a, b| b.length.cmp(&a.length));
        chains.truncate(20);
        chains
    }

    // ── Temporal Tracking ────────────────────────────────────────────

    /// Knowledge의 시간별 유효성 tracking.
    /// 각 Accepted 노드에 대해: 생성일, 마지막 검증일, 다음 검증 필요일.
    pub fn temporal_health(&self) -> Vec<TemporalStatus> {
        let now = chrono::Utc::now();
        let mut statuses = Vec::new();

        for node in self.nodes.values() {
            if node.status != NodeStatus::Accepted { continue; }

            let age_days = (now - node.created_at).num_days();
            let since_update = (now - node.updated_at).num_days();

            // Check for decay indicators in metadata/statement
            let has_decay_risk = node.statement.to_lowercase().contains("decay")
                || node.metadata.get("alpha_decay").and_then(|v| v.as_bool()).unwrap_or(false);

            // Revalidation schedule based on risk
            let revalidation_interval = if has_decay_risk { 14 } else { 30 };
            let days_until_revalidation = revalidation_interval - since_update;
            let urgency = if days_until_revalidation < 0 {
                "OVERDUE"
            } else if days_until_revalidation < 7 {
                "SOON"
            } else {
                "OK"
            };

            // Count supporting evidence
            let n_evidence = self.neighbors(&node.id, Direction::In, Some(&[EdgeKind::Support, EdgeKind::DerivedFrom]))
                .len();

            // Count contradicting nodes
            let n_contradicts = self.neighbors(&node.id, Direction::Both, Some(&[EdgeKind::Contradicts]))
                .len();

            let trigger = node.metadata.get("trigger")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            statuses.push(TemporalStatus {
                node_id: node.id,
                statement: node.statement.clone(),
                age_days,
                since_update_days: since_update,
                revalidation_interval,
                days_until_revalidation,
                urgency: urgency.into(),
                has_decay_risk,
                n_evidence,
                n_contradicts,
                trigger,
            });
        }

        statuses.sort_by_key(|s| s.days_until_revalidation);
        statuses
    }

    // ── Dependency Analysis ──────────────────────────────────────────

    /// 노드가 의존하는 모든 노드 (DependsOn 체인). 하나가 기각되면 영향받는 것들.
    pub fn dependency_tree(&self, node_id: &Uuid) -> Vec<(Uuid, usize, bool)> {
        // (id, depth, is_failed)
        let walked = self.walk(node_id, 5, Some(&[EdgeKind::DependsOn, EdgeKind::Support]));
        walked.iter().map(|(id, depth)| {
            let failed = self.nodes.get(id).map(|n| n.is_failed()).unwrap_or(false);
            (*id, *depth, failed)
        }).collect()
    }

    // ── [1] Verdict → Reason 상태 전파 ────────────────────────────────

    /// 실험 verdict 후 상위 Reason/Claim 상태를 자동 업데이트.
    /// 반환: (reason_id, reason_new_status, claim_id, claim_recommendation)
    pub fn propagate_verdict(&mut self, experiment_id: &Uuid) -> Option<VerdictPropagation> {
        // 1. Experiment → Reason (DependsOn 엣지 따라감)
        let reason_id = {
            let neighbors = self.neighbors(experiment_id, Direction::Out, Some(&[EdgeKind::DependsOn]));
            neighbors.first().map(|(id, _)| *id)?
        };

        // 2. Reason 하위의 모든 Experiment 상태 집계
        let experiments: Vec<(Uuid, NodeStatus)> = self
            .neighbors(&reason_id, Direction::In, Some(&[EdgeKind::DependsOn]))
            .iter()
            .filter_map(|(id, _)| {
                let n = self.nodes.get(id)?;
                if n.kind == NodeKind::Experiment {
                    Some((*id, n.status))
                } else {
                    None
                }
            })
            .collect();

        let n_total = experiments.len();
        let n_accepted = experiments.iter().filter(|(_, s)| *s == NodeStatus::Accepted).count();
        let n_weakened = experiments.iter().filter(|(_, s)| *s == NodeStatus::Weakened).count();
        let n_resolved = n_accepted + n_weakened;

        // 3. Reason 상태 결정 (다수결)
        let reason_status = if n_resolved == 0 {
            NodeStatus::Active
        } else if n_accepted > n_weakened {
            NodeStatus::Accepted // "Supported"
        } else {
            NodeStatus::Weakened
        };

        // Reason 상태 업데이트
        if let Some(reason) = self.nodes.get_mut(&reason_id) {
            reason.status = reason_status;
            reason.updated_at = chrono::Utc::now();
        }

        // 4. Reason → Claim (Support/Rebut 엣지 따라감)
        let claim_id = {
            let neighbors = self.neighbors(&reason_id, Direction::Out, Some(&[EdgeKind::Support, EdgeKind::Rebut]));
            neighbors.first().map(|(id, _)| *id)
        };

        // 5. Claim의 모든 Reason 상태 집계 → 추천
        let claim_recommendation = if let Some(cid) = claim_id {
            let reasons: Vec<(NodeStatus, EdgeKind)> = self
                .neighbors(&cid, Direction::In, Some(&[EdgeKind::Support, EdgeKind::Rebut]))
                .iter()
                .filter_map(|(id, edge)| {
                    let n = self.nodes.get(id)?;
                    if n.kind == NodeKind::Reason {
                        Some((n.status, edge.kind))
                    } else {
                        None
                    }
                })
                .collect();

            let n_support_accepted = reasons.iter()
                .filter(|(s, k)| *s == NodeStatus::Accepted && *k == EdgeKind::Support).count();
            let n_support_weakened = reasons.iter()
                .filter(|(s, k)| *s == NodeStatus::Weakened && *k == EdgeKind::Support).count();
            let n_rebut_accepted = reasons.iter()
                .filter(|(s, k)| *s == NodeStatus::Accepted && *k == EdgeKind::Rebut).count();
            let n_active = reasons.iter().filter(|(s, _)| *s == NodeStatus::Active).count();

            if n_active > 0 {
                "pending".to_string() // 아직 미완료 Reason 있음
            } else if n_rebut_accepted > 0 {
                "reject".to_string() // 반박이 지지됨 → 기각 추천
            } else if n_support_accepted > 0 && n_support_weakened == 0 {
                "accept".to_string() // 모든 지지 근거가 확인됨
            } else if n_support_accepted > n_support_weakened {
                "lean_accept".to_string() // 다수 지지
            } else {
                "lean_reject".to_string() // 다수 약화
            }
        } else {
            "no_claim".to_string()
        };

        Some(VerdictPropagation {
            experiment_id: *experiment_id,
            reason_id,
            reason_status: format!("{:?}", reason_status),
            n_experiments: n_total,
            n_accepted,
            n_weakened,
            claim_id,
            claim_recommendation,
        })
    }

    // ── [2] Claim 간 DependsOn 자동 감지 ────────────────────────────

    /// 새 Claim의 키워드와 기존 Accepted Claim들을 비교 → DependsOn 엣지 자동 생성.
    /// 반환: 생성된 (edge_id, target_claim_id, overlap_keywords) 목록
    pub fn detect_claim_dependencies(&mut self, new_claim_id: &Uuid) -> Vec<DependencyDetection> {
        let (new_kws, new_stmt) = {
            let node = match self.nodes.get(new_claim_id) {
                Some(n) if n.kind == NodeKind::Claim => n,
                _ => return Vec::new(),
            };
            let kws: HashSet<String> = node.keywords.iter().map(|k| k.to_lowercase()).collect();
            (kws, node.statement.clone())
        };

        if new_kws.is_empty() { return Vec::new(); }

        // 기존 Accepted Claim 수집
        let candidates: Vec<(Uuid, HashSet<String>, String)> = self.nodes.values()
            .filter(|n| {
                n.kind == NodeKind::Claim
                    && n.status == NodeStatus::Accepted
                    && n.id != *new_claim_id
            })
            .map(|n| {
                let kws: HashSet<String> = n.keywords.iter().map(|k| k.to_lowercase()).collect();
                (n.id, kws, n.statement.clone())
            })
            .collect();

        let mut results = Vec::new();

        for (cid, cand_kws, cand_stmt) in &candidates {
            let overlap: Vec<String> = new_kws.intersection(cand_kws).cloned().collect();
            let union_size = new_kws.union(cand_kws).count();
            let jaccard = if union_size > 0 { overlap.len() as f64 / union_size as f64 } else { 0.0 };

            // Jaccard > 0.3 이고 overlap 2개 이상
            if jaccard > 0.3 && overlap.len() >= 2 {
                // 이미 DependsOn 엣지가 있는지 확인
                let already_exists = self.edges.values().any(|e| {
                    e.kind == EdgeKind::DependsOn
                        && ((e.source == *new_claim_id && e.target == *cid)
                            || (e.source == *cid && e.target == *new_claim_id))
                });

                if !already_exists {
                    let edge_id = self.add_edge(
                        Edge::new(*new_claim_id, *cid, EdgeKind::DependsOn)
                            .with_weight(jaccard)
                            .with_note(format!("자동 감지: 키워드 겹침({})", overlap.join(",")))
                    );

                    results.push(DependencyDetection {
                        edge_id,
                        source_id: *new_claim_id,
                        source_statement: new_stmt.clone(),
                        target_id: *cid,
                        target_statement: cand_stmt.clone(),
                        overlap_keywords: overlap,
                        jaccard,
                    });
                }
            }
        }

        results.sort_by(|a, b| b.jaccard.partial_cmp(&a.jaccard).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    // ── [3] Knowledge 승격 시 관계 엣지 ─────────────────────────────

    /// Claim이 Accept될 때 기존 Accepted Claim들과 관계 엣지 자동 생성.
    /// - 같은 방향 + 키워드 겹침 → Support
    /// - 더 구체적 (키워드 포함관계) → Refines
    /// - 방향 충돌 → Contradicts
    pub fn on_accept(&mut self, claim_id: &Uuid) -> Vec<AcceptRelation> {
        let (kws, _stmt, is_reversal) = {
            let node = match self.nodes.get(claim_id) {
                Some(n) if n.kind == NodeKind::Claim => n,
                _ => return Vec::new(),
            };
            let kws: HashSet<String> = node.keywords.iter()
                .filter(|k| !["validated", "disproven", "gap_derived", "auto_generated"].contains(&k.as_str()))
                .map(|k| k.to_lowercase())
                .collect();
            let is_rev = node.statement.contains("reversal") || node.statement.contains("반전") || node.statement.contains("회귀");
            (kws, node.statement.clone(), is_rev)
        };

        if kws.is_empty() { return Vec::new(); }

        // 다른 Accepted Claim들
        let others: Vec<(Uuid, HashSet<String>, String, bool)> = self.nodes.values()
            .filter(|n| {
                n.kind == NodeKind::Claim
                    && n.status == NodeStatus::Accepted
                    && n.id != *claim_id
            })
            .map(|n| {
                let k: HashSet<String> = n.keywords.iter()
                    .filter(|k| !["validated", "disproven", "gap_derived", "auto_generated"].contains(&k.as_str()))
                    .map(|k| k.to_lowercase())
                    .collect();
                let rev = n.statement.contains("reversal") || n.statement.contains("반전") || n.statement.contains("회귀");
                (n.id, k, n.statement.clone(), rev)
            })
            .collect();

        let mut relations = Vec::new();

        for (oid, okws, ostmt, o_rev) in &others {
            let overlap: Vec<String> = kws.intersection(okws).cloned().collect();
            if overlap.len() < 2 { continue; }

            // 이미 관계 엣지가 있는지 확인
            let has_existing = self.edges.values().any(|e| {
                matches!(e.kind, EdgeKind::Support | EdgeKind::Refines | EdgeKind::Contradicts)
                    && ((e.source == *claim_id && e.target == *oid)
                        || (e.source == *oid && e.target == *claim_id))
            });
            if has_existing { continue; }

            // 관계 종류 결정
            let (edge_kind, relation_type) = if is_reversal != *o_rev {
                // 방향 충돌
                (EdgeKind::Contradicts, "contradicts")
            } else if kws.is_subset(okws) || okws.is_subset(&kws) {
                // 포함 관계 → Refines (더 구체적인 것 → 더 일반적인 것)
                (EdgeKind::Refines, "refines")
            } else {
                // 같은 방향 + 키워드 겹침
                (EdgeKind::Support, "supports")
            };

            let (source, target) = if edge_kind == EdgeKind::Refines {
                // 키워드가 더 많은 쪽이 더 구체적 → source
                if kws.len() >= okws.len() { (*claim_id, *oid) } else { (*oid, *claim_id) }
            } else {
                (*claim_id, *oid)
            };

            let edge_id = self.add_edge(
                Edge::new(source, target, edge_kind)
                    .with_note(format!("승격 시 자동: {} (겹침: {})", relation_type, overlap.join(",")))
            );

            relations.push(AcceptRelation {
                edge_id,
                other_id: *oid,
                other_statement: ostmt.clone(),
                relation_type: relation_type.to_string(),
                overlap_keywords: overlap,
            });
        }

        relations
    }

    /// 이 노드가 기각되면 영향받는 Knowledge 목록.
    pub fn impact_analysis(&self, node_id: &Uuid) -> Vec<Uuid> {
        // 이 노드를 Support/DependsOn하는 노드들 (역방향)
        let mut impacted = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(*node_id);
        visited.insert(*node_id);

        while let Some(current) = queue.pop_front() {
            // Find nodes that depend ON this node
            if let Some(edge_ids) = self.reverse_adj.get(&current) {
                for eid in edge_ids {
                    if let Some(edge) = self.edges.get(eid) {
                        if matches!(edge.kind, EdgeKind::Support | EdgeKind::DependsOn | EdgeKind::DerivedFrom) {
                            if visited.insert(edge.source) {
                                if let Some(n) = self.nodes.get(&edge.source) {
                                    if n.status == NodeStatus::Accepted {
                                        impacted.push(edge.source);
                                    }
                                }
                                queue.push_back(edge.source);
                            }
                        }
                    }
                }
            }
        }

        impacted
    }
}

// ── Result Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct CausalChain {
    pub nodes: Vec<ChainNode>,
    pub edge_types: Vec<String>,
    pub length: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChainNode {
    pub id: Uuid,
    pub kind: String,
    pub statement: String,
    pub status: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TemporalStatus {
    pub node_id: Uuid,
    pub statement: String,
    pub age_days: i64,
    pub since_update_days: i64,
    pub revalidation_interval: i64,
    pub days_until_revalidation: i64,
    pub urgency: String,
    pub has_decay_risk: bool,
    pub n_evidence: usize,
    pub n_contradicts: usize,
    pub trigger: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VerdictPropagation {
    pub experiment_id: Uuid,
    pub reason_id: Uuid,
    pub reason_status: String,
    pub n_experiments: usize,
    pub n_accepted: usize,
    pub n_weakened: usize,
    pub claim_id: Option<Uuid>,
    pub claim_recommendation: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DependencyDetection {
    pub edge_id: Uuid,
    pub source_id: Uuid,
    pub source_statement: String,
    pub target_id: Uuid,
    pub target_statement: String,
    pub overlap_keywords: Vec<String>,
    pub jaccard: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AcceptRelation {
    pub edge_id: Uuid,
    pub other_id: Uuid,
    pub other_statement: String,
    pub relation_type: String,
    pub overlap_keywords: Vec<String>,
}

impl Default for AmureGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphSummary {
    pub n_nodes: usize,
    pub n_edges: usize,
    pub n_failed: usize,
    pub node_kinds: HashMap<String, usize>,
    pub edge_kinds: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_claim(statement: &str, kw: &[&str]) -> Node {
        Node::new(NodeKind::Claim, statement.into(), kw.iter().map(|s| s.to_string()).collect())
    }

    fn make_reason(statement: &str, kw: &[&str]) -> Node {
        Node::new(NodeKind::Reason, statement.into(), kw.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn test_add_and_get() {
        let mut g = AmureGraph::new();
        let node = make_claim("OI는 momentum 선행지표", &["OI", "momentum"]);
        let id = g.add_node(node);
        assert!(g.get_node(&id).is_some());
        assert_eq!(g.node_count(), 1);
    }

    #[test]
    fn test_remove_node_cascades_edges() {
        let mut g = AmureGraph::new();
        let c = g.add_node(make_claim("claim", &[]));
        let r = g.add_node(make_reason("reason", &[]));
        g.add_edge(Edge::new(r, c, EdgeKind::Support));
        assert_eq!(g.edge_count(), 1);

        g.remove_node(&c);
        assert_eq!(g.node_count(), 1);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_neighbors() {
        let mut g = AmureGraph::new();
        let c = g.add_node(make_claim("claim", &[]));
        let r1 = g.add_node(make_reason("support", &[]));
        let r2 = g.add_node(make_reason("rebut", &[]));
        g.add_edge(Edge::new(r1, c, EdgeKind::Support));
        g.add_edge(Edge::new(r2, c, EdgeKind::Rebut));

        // Incoming to claim
        let neighbors = g.neighbors(&c, Direction::In, None);
        assert_eq!(neighbors.len(), 2);

        // Filter support only
        let support_only = g.neighbors(&c, Direction::In, Some(&[EdgeKind::Support]));
        assert_eq!(support_only.len(), 1);
    }

    #[test]
    fn test_walk_bfs() {
        let mut g = AmureGraph::new();
        let c = g.add_node(make_claim("claim", &[]));
        let r = g.add_node(make_reason("reason", &[]));
        let e = g.add_node(Node::new(NodeKind::Evidence, "evidence".into(), vec![]));
        g.add_edge(Edge::new(r, c, EdgeKind::Support));
        g.add_edge(Edge::new(e, r, EdgeKind::DerivedFrom));

        // Walk from claim, 2 hops
        let walked = g.walk(&c, 2, None);
        assert_eq!(walked.len(), 3); // claim(0), reason(1), evidence(2)

        // Walk from claim, 1 hop
        let walked_1 = g.walk(&c, 1, None);
        assert_eq!(walked_1.len(), 2); // claim(0), reason(1)
    }

    #[test]
    fn test_subgraph() {
        let mut g = AmureGraph::new();
        let c = g.add_node(make_claim("claim", &[]));
        let r = g.add_node(make_reason("reason", &[]));
        let other = g.add_node(make_claim("other", &[]));
        g.add_edge(Edge::new(r, c, EdgeKind::Support));

        let (nodes, edges) = g.subgraph(&[c, r]);
        assert_eq!(nodes.len(), 2);
        assert_eq!(edges.len(), 1);

        // other is not in subgraph
        let (nodes2, _) = g.subgraph(&[c, other]);
        assert_eq!(nodes2.len(), 2);
    }

    #[test]
    fn test_summary() {
        let mut g = AmureGraph::new();
        g.add_node(make_claim("c1", &[]));
        g.add_node(make_reason("r1", &[]).with_status(NodeStatus::Weakened));
        let s = g.summary();
        assert_eq!(s.n_nodes, 2);
        assert_eq!(s.n_failed, 1);
    }
}
