/// Node — 지식 그래프의 노드. 모든 지식은 한 문장 명제.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    Claim,
    Reason,
    Evidence,
    Experiment,
    Fact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    Draft,
    Active,
    Accepted,
    Rejected,
    Weakened,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
    pub kind: NodeKind,
    pub statement: String,
    pub keywords: Vec<String>,
    pub metadata: serde_json::Value,
    pub status: NodeStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Node {
    pub fn new(kind: NodeKind, statement: String, keywords: Vec<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            kind,
            statement,
            keywords,
            metadata: serde_json::Value::Null,
            status: NodeStatus::Draft,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_id(mut self, id: Uuid) -> Self {
        self.id = id;
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn with_status(mut self, status: NodeStatus) -> Self {
        self.status = status;
        self
    }

    pub fn is_failed(&self) -> bool {
        matches!(self.status, NodeStatus::Rejected | NodeStatus::Weakened)
    }

    /// 노드의 모든 텍스트를 소문자 토큰으로 반환 (검색용)
    pub fn tokens(&self) -> Vec<String> {
        let text = format!("{} {}", self.statement, self.keywords.join(" "));
        tokenize(&text)
    }
}

/// 한/영 혼합 텍스트 토크나이저
pub fn tokenize(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut tokens = Vec::new();
    let mut current = String::new();

    let is_korean = |c: char| ('\u{AC00}'..='\u{D7A3}').contains(&c);
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';

    for ch in lower.chars() {
        if is_word(ch) || is_korean(ch) {
            if !current.is_empty() {
                let last = current.chars().last().unwrap();
                if (is_korean(last) && is_word(ch)) || (is_word(last) && is_korean(ch)) {
                    if current.len() >= 2 {
                        tokens.push(current.clone());
                    }
                    current.clear();
                }
            }
            current.push(ch);
        } else {
            if current.len() >= 2 {
                tokens.push(current.clone());
            }
            current.clear();
        }
    }
    if current.len() >= 2 {
        tokens.push(current);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let node = Node::new(
            NodeKind::Claim,
            "OI는 momentum의 선행지표다".into(),
            vec!["OI".into(), "momentum".into()],
        );
        assert_eq!(node.kind, NodeKind::Claim);
        assert_eq!(node.status, NodeStatus::Draft);
        assert!(!node.is_failed());
    }

    #[test]
    fn test_failed_status() {
        let node = Node::new(NodeKind::Claim, "test".into(), vec![])
            .with_status(NodeStatus::Rejected);
        assert!(node.is_failed());
    }

    #[test]
    fn test_tokenize() {
        let tokens = tokenize("OI momentum은 크립토에서 continuation alpha가 있다");
        assert!(tokens.contains(&"momentum".to_string()));
        assert!(tokens.contains(&"continuation".to_string()));
        assert!(tokens.contains(&"alpha".to_string()));
    }

    #[test]
    fn test_tokenize_underscore() {
        let tokens = tokenize("open_interest cross_sectional");
        assert!(tokens.contains(&"open_interest".to_string()));
        assert!(tokens.contains(&"cross_sectional".to_string()));
    }
}
