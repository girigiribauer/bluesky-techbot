mod feed;
mod hackernews;

use crate::config::Config;
use crate::db;
use crate::http;
use anyhow::Result;
use sqlx::SqlitePool;

pub async fn collect(config: &Config, pool: &SqlitePool) -> Result<usize> {
    let client = http::client()?;

    let mut articles = Vec::new();
    articles.extend(feed::fetch(&client, "zenn", &config.sources.zenn).await?);
    articles.extend(feed::fetch(&client, "hatena", &config.sources.hatena).await?);
    articles.extend(
        hackernews::fetch(
            &client,
            config.hackernews.min_points,
            config.hackernews.window_hours,
        )
        .await?,
    );

    let mut added = 0;
    for article in &mut articles {
        article.url = crate::url::normalize(&article.url);
        if db::insert(pool, article).await? {
            added += 1;
        }
    }
    Ok(added)
}
