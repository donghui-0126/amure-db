/// Persistence — JSON 파일 기반 저장/로드.
/// nodes.json + edges.json → AmureGraph 복원.

use std::path::Path;
use crate::graph::AmureGraph;
use crate::node::Node;
use crate::edge::Edge;

impl AmureGraph {
    /// 디렉토리에 저장 (atomic write: .tmp → rename)
    pub fn save(&self, dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(dir)?;

        let nodes: Vec<&Node> = self.nodes.values().collect();
        let edges: Vec<&Edge> = self.edges.values().collect();

        // Write to tmp then rename (atomic)
        let nodes_tmp = dir.join("nodes.json.tmp");
        let edges_tmp = dir.join("edges.json.tmp");
        let nodes_path = dir.join("nodes.json");
        let edges_path = dir.join("edges.json");

        std::fs::write(&nodes_tmp, serde_json::to_string_pretty(&nodes)?)?;
        std::fs::write(&edges_tmp, serde_json::to_string_pretty(&edges)?)?;

        std::fs::rename(&nodes_tmp, &nodes_path)?;
        std::fs::rename(&edges_tmp, &edges_path)?;

        Ok(())
    }

    /// 디렉토리에서 로드 + adjacency 재구축
    pub fn load(dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let mut graph = Self::new();

        let nodes_path = dir.join("nodes.json");
        let edges_path = dir.join("edges.json");

        if nodes_path.exists() {
            let content = std::fs::read_to_string(&nodes_path)?;
            let nodes: Vec<Node> = serde_json::from_str(&content)?;
            for node in nodes {
                graph.add_node(node);
            }
        }

        if edges_path.exists() {
            let content = std::fs::read_to_string(&edges_path)?;
            let edges: Vec<Edge> = serde_json::from_str(&content)?;
            for edge in edges {
                graph.add_edge(edge);
            }
        }

        Ok(graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edge::EdgeKind;
    use crate::node::{NodeKind, NodeStatus};

    #[test]
    fn test_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut g = AmureGraph::new();

        let c = g.add_node(Node::new(
            NodeKind::Claim, "test claim".into(), vec!["test".into()],
        ));
        let r = g.add_node(
            Node::new(NodeKind::Reason, "test reason".into(), vec![])
                .with_status(NodeStatus::Weakened)
        );
        g.add_edge(Edge::new(r, c, EdgeKind::Support).with_note("test edge".into()));

        g.save(dir.path()).unwrap();

        let g2 = AmureGraph::load(dir.path()).unwrap();
        assert_eq!(g2.node_count(), 2);
        assert_eq!(g2.edge_count(), 1);
        assert!(g2.get_node(&c).is_some());
        assert_eq!(g2.get_node(&r).unwrap().status, NodeStatus::Weakened);

        // Adjacency works after load
        let neighbors = g2.neighbors(&c, crate::graph::Direction::In, None);
        assert_eq!(neighbors.len(), 1);
    }

    #[test]
    fn test_load_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let g = AmureGraph::load(dir.path()).unwrap();
        assert_eq!(g.node_count(), 0);
    }
}
