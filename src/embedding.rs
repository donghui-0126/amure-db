/// Embedding — OpenAI text-embedding-3-small 기반 의미 검색.
/// OPENAI_API_KEY 환경변수 필요. 없으면 graceful fallback.

use serde::{Deserialize, Serialize};

/// OpenAI embedding API 호출. 1536차원 Vec<f32> 반환.
pub async fn get_embedding(text: &str) -> Result<Vec<f32>, Box<dyn std::error::Error + Send + Sync>> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| "OPENAI_API_KEY not set")?;

    let client = reqwest::Client::new();

    #[derive(Serialize)]
    struct Req<'a> {
        model: &'a str,
        input: &'a str,
    }

    let resp = client
        .post("https://api.openai.com/v1/embeddings")
        .bearer_auth(&api_key)
        .json(&Req { model: "text-embedding-3-small", input: text })
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("OpenAI API error {}: {}", status, body).into());
    }

    #[derive(Deserialize)]
    struct EmbeddingData {
        embedding: Vec<f32>,
    }
    #[derive(Deserialize)]
    struct EmbeddingResp {
        data: Vec<EmbeddingData>,
    }

    let parsed: EmbeddingResp = resp.json().await?;
    parsed.data.into_iter().next()
        .map(|d| d.embedding)
        .ok_or_else(|| "Empty embedding response".into())
}

/// 배치 임베딩. OpenAI는 최대 2048개 입력 지원.
pub async fn get_embeddings_batch(texts: &[String]) -> Result<Vec<Vec<f32>>, Box<dyn std::error::Error + Send + Sync>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| "OPENAI_API_KEY not set")?;

    let client = reqwest::Client::new();

    #[derive(Serialize)]
    struct Req<'a> {
        model: &'a str,
        input: &'a [String],
    }

    let resp = client
        .post("https://api.openai.com/v1/embeddings")
        .bearer_auth(&api_key)
        .json(&Req { model: "text-embedding-3-small", input: texts })
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("OpenAI API error {}: {}", status, body).into());
    }

    #[derive(Deserialize)]
    struct EmbeddingData {
        embedding: Vec<f32>,
        index: usize,
    }
    #[derive(Deserialize)]
    struct EmbeddingResp {
        data: Vec<EmbeddingData>,
    }

    let mut parsed: EmbeddingResp = resp.json().await?;
    // OpenAI may return results out of order; sort by index
    parsed.data.sort_by_key(|d| d.index);
    Ok(parsed.data.into_iter().map(|d| d.embedding).collect())
}

/// 코사인 유사도. 두 벡터 길이가 다르면 0.0 반환.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut mag_a = 0.0f64;
    let mut mag_b = 0.0f64;
    for i in 0..a.len() {
        let ai = a[i] as f64;
        let bi = b[i] as f64;
        dot += ai * bi;
        mag_a += ai * ai;
        mag_b += bi * bi;
    }
    let denom = mag_a.sqrt() * mag_b.sqrt();
    if denom == 0.0 { 0.0 } else { dot / denom }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_identical() {
        let v = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_empty() {
        let sim = cosine_similarity(&[], &[]);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_different_lengths() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }
}
