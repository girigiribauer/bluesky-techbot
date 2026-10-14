#[macro_use]
mod log;

mod bluesky;
mod config;
mod db;
mod extract;
mod http;
mod judge;
mod pipeline;
mod post;
mod sources;
mod url;
mod web;

use anyhow::Result;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::load("config.toml")?;
    let pool = db::connect(&config.db.path).await?;

    if std::env::args().any(|arg| arg == "--post-one") {
        pipeline::post_one(&config, &pool).await?;
        return Ok(());
    }

    if std::env::args().any(|arg| arg == "--once") {
        pipeline::run(&config, &pool).await;
        log!("一巡しました");
        return Ok(());
    }

    let posting = !std::env::args().any(|arg| arg == "--no-post");

    let web_only = std::env::args().any(|arg| arg == "--web-only");
    if !web_only && posting {
        match pipeline::sync_posted(&config, &pool).await {
            Ok(_) => {}
            Err(error) => {
                log_error!("投稿済みの照合に失敗したため、投稿は行いません: {error:#}");
                return web::serve(config.web.port, pool).await;
            }
        }

        let poster_config = config.clone();
        let poster_pool = pool.clone();
        tokio::spawn(async move {
            let interval = Duration::from_secs(poster_config.posting.interval_minutes * 60);
            loop {
                pipeline::wait_until_due(&poster_config, &poster_pool).await;
                pipeline::post_batch(&poster_config, &poster_pool).await;
                pipeline::sleep_watching(interval).await;
            }
        });
    }

    if !web_only {
        let worker_config = config.clone();
        let worker_pool = pool.clone();
        tokio::spawn(async move {
            let interval = Duration::from_secs(worker_config.collect.interval_minutes * 60);
            loop {
                pipeline::run(&worker_config, &worker_pool).await;
                pipeline::sleep_watching(interval).await;
            }
        });
    }

    web::serve(config.web.port, pool).await
}
