use crate::db;
use anyhow::Result;
use serde::Deserialize;

#[derive(Deserialize)]
struct Response {
    hits: Vec<Hit>,
}

#[derive(Deserialize)]
struct Hit {
    #[serde(rename = "objectID")]
    object_id: String,
    title: Option<String>,
    url: Option<String>,
    points: Option<i64>,
    created_at: Option<String>,
}

pub async fn fetch(
    client: &reqwest::Client,
    min_points: i64,
    window_hours: i64,
) -> Result<Vec<db::NewArticle>> {
    let since = (chrono::Utc::now() - chrono::Duration::hours(window_hours)).timestamp();
    let filters = format!("created_at_i>{since},points>{min_points}");
    let response: Response = client
        .get("https://hn.algolia.com/api/v1/search_by_date")
        .query(&[
            ("tags", "story"),
            ("numericFilters", filters.as_str()),
            ("hitsPerPage", "100"),
        ])
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() || error.is_connect() || error.is_request() {
                anyhow::Error::new(crate::extract::Transient(format!(
                    "接続できません: {error}"
                )))
            } else {
                anyhow::Error::new(error)
            }
        })?
        .error_for_status()?
        .json()
        .await?;

    let mut articles = Vec::new();
    for hit in response.hits {
        let Some(title) = hit.title else {
            continue;
        };
        let url = hit
            .url
            .unwrap_or_else(|| format!("https://news.ycombinator.com/item?id={}", hit.object_id));
        articles.push(db::NewArticle {
            source: "hackernews".to_string(),
            url,
            title,
            score: hit.points,
            published_at: hit
                .created_at
                .map(|at| crate::clock::utc_text_of(&at).unwrap_or(at)),
        });
    }
    Ok(articles)
}
