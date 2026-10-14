use super::{Content, get_text, normalize};
use crate::config::Limits;
use anyhow::{Result, anyhow};
use dom_smoothie::Readability;

pub async fn extract(client: &reqwest::Client, url: &str, limits: &Limits) -> Result<Content> {
    let html = get_text(client, url).await?;
    let card = super::card(&html, url);
    let source = url.to_string();
    let extracted = tokio::task::spawn_blocking(move || -> Result<String> {
        let mut readability = Readability::new(html.as_str(), Some(&source), None)?;
        let article = readability.parse()?;
        Ok(article.text_content.to_string())
    })
    .await??;

    let normalized = normalize(&extracted);
    let length = normalized.chars().count();
    if length < limits.min_body_chars {
        return Err(anyhow!("本文が短すぎます（{length}字）"));
    }

    let truncated = length > limits.body_chars;
    let text = if truncated {
        normalized.chars().take(limits.body_chars).collect()
    } else {
        normalized
    };
    Ok(Content::Text {
        text,
        truncated,
        card,
    })
}
