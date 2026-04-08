/// Edge — 노드 간 관계. 방향성 있음 (source → target).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    /// 교집합 있음 (조건/메커니즘 공유)
    Reference,
    /// source가 target을 포함하는 상위개념
    Superset,
    /// source가 target의 부분집합
    Subset,
    /// 완전히 다른 영역, 겹침 없음
    Orthogonal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: Uuid,
    pub source: Uuid,
    pub target: Uuid,
    pub kind: EdgeKind,
    pub reason: String,
    pub created_at: DateTime<Utc>,
}

impl Edge {
    pub fn new(source: Uuid, target: Uuid, kind: EdgeKind, reason: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            source,
            target,
            kind,
            reason,
            created_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_creation() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let edge = Edge::new(a, b, EdgeKind::Reference, "공통 시그널 사용".into());
        assert_eq!(edge.source, a);
        assert_eq!(edge.target, b);
        assert_eq!(edge.kind, EdgeKind::Reference);
        assert_eq!(edge.reason, "공통 시그널 사용");
    }

    #[test]
    fn test_edge_kinds() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        assert_eq!(Edge::new(a, b, EdgeKind::Superset, "r".into()).kind, EdgeKind::Superset);
        assert_eq!(Edge::new(a, b, EdgeKind::Subset, "r".into()).kind, EdgeKind::Subset);
        assert_eq!(Edge::new(a, b, EdgeKind::Orthogonal, "r".into()).kind, EdgeKind::Orthogonal);
    }
}
