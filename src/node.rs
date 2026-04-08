/// Node — 지식 그래프의 노드. Hypothesis 단일 종류.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    Hypothesis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    Draft,
    Accept,
    Decline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExperimentKind {
    Universe,
    Regime,
    Temporal,
    Combo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    Support,
    Rebut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    pub id: Uuid,
    pub kind: ExperimentKind,
    pub target: String,
    pub result: serde_json::Value,
    pub verdict: Verdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
    pub kind: NodeKind,
    pub statement: String,
    #[serde(rename = "abstract_", alias = "abstract_")]
    pub abstract_: String,
    pub discussion: String,
    pub status: NodeStatus,
    #[serde(default)]
    pub experiments: Vec<Experiment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Node {
    pub fn new(statement: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            kind: NodeKind::Hypothesis,
            statement,
            abstract_: String::new(),
            discussion: String::new(),
            status: NodeStatus::Draft,
            experiments: Vec::new(),
            embedding: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// 임베딩용 텍스트: statement + abstract_ 결합
    pub fn embed_text(&self) -> String {
        if self.abstract_.is_empty() {
            self.statement.clone()
        } else {
            format!("{} {}", self.statement, self.abstract_)
        }
    }

    /// 노드의 모든 텍스트를 소문자 토큰으로 반환 (검색용)
    pub fn tokens(&self) -> Vec<String> {
        let text = format!("{} {} {}", self.statement, self.abstract_, self.discussion);
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
        let node = Node::new("OI는 momentum의 선행지표다".into());
        assert_eq!(node.kind, NodeKind::Hypothesis);
        assert_eq!(node.status, NodeStatus::Draft);
        assert!(node.abstract_.is_empty());
        assert!(node.experiments.is_empty());
    }

    #[test]
    fn test_node_status() {
        let mut node = Node::new("test".into());
        node.status = NodeStatus::Accept;
        assert_eq!(node.status, NodeStatus::Accept);
        node.status = NodeStatus::Decline;
        assert_eq!(node.status, NodeStatus::Decline);
    }

    #[test]
    fn test_embed_text() {
        let mut node = Node::new("statement".into());
        assert_eq!(node.embed_text(), "statement");
        node.abstract_ = "abstract summary".into();
        assert_eq!(node.embed_text(), "statement abstract summary");
    }

    #[test]
    fn test_experiment() {
        let exp = Experiment {
            id: Uuid::new_v4(),
            kind: ExperimentKind::Universe,
            target: "BTC,ETH".into(),
            result: serde_json::json!({"IR": 0.5, "t_stat": 2.1}),
            verdict: Verdict::Support,
            note: Some("좋은 결과".into()),
        };
        assert_eq!(exp.kind, ExperimentKind::Universe);
        assert_eq!(exp.verdict, Verdict::Support);
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
