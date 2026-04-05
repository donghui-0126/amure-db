/// Edge — 노드 간 관계. 방향성 있음 (source → target).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    /// Claim을 지지하는 근거
    Support,
    /// Claim을 반박하는 반론
    Rebut,
    /// A가 참이려면 B가 먼저 참이어야 함
    DependsOn,
    /// A와 B는 동시에 참일 수 없음
    Contradicts,
    /// A는 B의 더 구체적인 버전
    Refines,
    /// A는 B 실험/분석에서 파생됨
    DerivedFrom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: Uuid,
    pub source: Uuid,
    pub target: Uuid,
    pub kind: EdgeKind,
    pub weight: f64,
    pub note: String,
    pub created_at: DateTime<Utc>,
}

impl Edge {
    pub fn new(source: Uuid, target: Uuid, kind: EdgeKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            source,
            target,
            kind,
            weight: 1.0,
            note: String::new(),
            created_at: Utc::now(),
        }
    }

    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_note(mut self, note: String) -> Self {
        self.note = note;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_creation() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let edge = Edge::new(a, b, EdgeKind::Support).with_weight(0.8).with_note("test".into());
        assert_eq!(edge.source, a);
        assert_eq!(edge.target, b);
        assert_eq!(edge.kind, EdgeKind::Support);
        assert_eq!(edge.weight, 0.8);
    }
}
