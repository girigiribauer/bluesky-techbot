use crate::db;
use anyhow::Result;

pub async fn fetch(
    client: &reqwest::Client,
    source: &str,
    url: &str,
) -> Result<Vec<db::NewArticle>> {
    let bytes = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let parsed = feed_rs::parser::parse(&bytes[..])?;

    let mut articles = Vec::new();
    for entry in parsed.entries {
        let Some(link) = entry.links.into_iter().next() else {
            continue;
        };
        let Some(title) = entry.title else {
            continue;
        };
        articles.push(db::NewArticle {
            source: source.to_string(),
            url: link.href,
            title: title.content,
            score: None,
            published_at: entry.published.map(|at| at.to_rfc3339()),
        });
    }
    Ok(articles)
}
