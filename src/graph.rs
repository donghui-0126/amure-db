/// AmureGraph — 인메모리 그래프 엔진.
/// Adjacency list 기반, BFS walk, 노드/엣지 CRUD.

use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

use crate::edge::{Edge, EdgeKind};
use crate::node::{Node, NodeStatus};

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
    /// exclude_orthogonal이 true이면 Orthogonal 엣지를 건너뛰지 않음.
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

    /// Orthogonal 엣지 제외 walk
    pub fn walk_exclude_orthogonal(
        &self,
        start: &Uuid,
        max_hops: usize,
    ) -> Vec<(Uuid, usize)> {
        self.walk(start, max_hops, Some(&[EdgeKind::Reference, EdgeKind::Superset, EdgeKind::Subset]))
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
        let mut status_counts = HashMap::new();
        let mut total_experiments = 0usize;
        for node in self.nodes.values() {
            *status_counts.entry(format!("{:?}", node.status)).or_insert(0usize) += 1;
            total_experiments += node.experiments.len();
        }
        let mut edge_counts = HashMap::new();
        for edge in self.edges.values() {
            *edge_counts.entry(format!("{:?}", edge.kind)).or_insert(0usize) += 1;
        }

        GraphSummary {
            n_nodes: self.nodes.len(),
            n_edges: self.edges.len(),
            n_experiments: total_experiments,
            status_counts,
            edge_kinds: edge_counts,
        }
    }
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
    pub n_experiments: usize,
    pub status_counts: HashMap<String, usize>,
    pub edge_kinds: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edge::EdgeKind;

    fn make_hypothesis(statement: &str) -> Node {
        Node::new(statement.into())
    }

    #[test]
    fn test_add_and_get() {
        let mut g = AmureGraph::new();
        let node = make_hypothesis("OI는 momentum 선행지표");
        let id = g.add_node(node);
        assert!(g.get_node(&id).is_some());
        assert_eq!(g.node_count(), 1);
    }

    #[test]
    fn test_remove_node_cascades_edges() {
        let mut g = AmureGraph::new();
        let c = g.add_node(make_hypothesis("hypothesis 1"));
        let r = g.add_node(make_hypothesis("hypothesis 2"));
        g.add_edge(Edge::new(r, c, EdgeKind::Reference, "관련".into()));
        assert_eq!(g.edge_count(), 1);

        g.remove_node(&c);
        assert_eq!(g.node_count(), 1);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_neighbors() {
        let mut g = AmureGraph::new();
        let c = g.add_node(make_hypothesis("hypothesis 1"));
        let r1 = g.add_node(make_hypothesis("hypothesis 2"));
        let r2 = g.add_node(make_hypothesis("hypothesis 3"));
        g.add_edge(Edge::new(r1, c, EdgeKind::Reference, "ref".into()));
        g.add_edge(Edge::new(r2, c, EdgeKind::Superset, "sup".into()));

        // Incoming to c
        let neighbors = g.neighbors(&c, Direction::In, None);
        assert_eq!(neighbors.len(), 2);

        // Filter Reference only
        let ref_only = g.neighbors(&c, Direction::In, Some(&[EdgeKind::Reference]));
        assert_eq!(ref_only.len(), 1);
    }

    #[test]
    fn test_walk_bfs() {
        let mut g = AmureGraph::new();
        let a = g.add_node(make_hypothesis("h1"));
        let b = g.add_node(make_hypothesis("h2"));
        let c = g.add_node(make_hypothesis("h3"));
        g.add_edge(Edge::new(b, a, EdgeKind::Reference, "r".into()));
        g.add_edge(Edge::new(c, b, EdgeKind::Subset, "s".into()));

        // Walk from a, 2 hops
        let walked = g.walk(&a, 2, None);
        assert_eq!(walked.len(), 3);

        // Walk from a, 1 hop
        let walked_1 = g.walk(&a, 1, None);
        assert_eq!(walked_1.len(), 2);
    }

    #[test]
    fn test_walk_exclude_orthogonal() {
        let mut g = AmureGraph::new();
        let a = g.add_node(make_hypothesis("h1"));
        let b = g.add_node(make_hypothesis("h2"));
        let c = g.add_node(make_hypothesis("h3"));
        g.add_edge(Edge::new(b, a, EdgeKind::Reference, "r".into()));
        g.add_edge(Edge::new(c, a, EdgeKind::Orthogonal, "o".into()));

        // Walk excluding orthogonal — should NOT reach c
        let walked = g.walk_exclude_orthogonal(&a, 2);
        assert_eq!(walked.len(), 2); // a + b only

        // Walk including all — should reach c
        let walked_all = g.walk(&a, 2, None);
        assert_eq!(walked_all.len(), 3); // a + b + c
    }

    #[test]
    fn test_subgraph() {
        let mut g = AmureGraph::new();
        let a = g.add_node(make_hypothesis("h1"));
        let b = g.add_node(make_hypothesis("h2"));
        let c = g.add_node(make_hypothesis("h3"));
        g.add_edge(Edge::new(b, a, EdgeKind::Reference, "r".into()));

        let (nodes, edges) = g.subgraph(&[a, b]);
        assert_eq!(nodes.len(), 2);
        assert_eq!(edges.len(), 1);

        // c is not in subgraph
        let (nodes2, _) = g.subgraph(&[a, c]);
        assert_eq!(nodes2.len(), 2);
    }

    #[test]
    fn test_summary() {
        let mut g = AmureGraph::new();
        g.add_node(make_hypothesis("h1"));
        let mut h2 = make_hypothesis("h2");
        h2.status = NodeStatus::Decline;
        g.add_node(h2);
        let s = g.summary();
        assert_eq!(s.n_nodes, 2);
        assert_eq!(s.n_experiments, 0);
    }

    #[test]
    fn test_summary_counts_experiments() {
        let mut g = AmureGraph::new();
        let mut h = make_hypothesis("h1");
        h.experiments.push(crate::node::Experiment {
            id: uuid::Uuid::new_v4(),
            kind: crate::node::ExperimentKind::Universe,
            target: "BTC".into(),
            result: serde_json::json!({}),
            verdict: crate::node::Verdict::Support,
            note: None,
        });
        g.add_node(h);
        let s = g.summary();
        assert_eq!(s.n_experiments, 1);
    }
}
