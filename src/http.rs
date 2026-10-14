use anyhow::Result;
use std::time::Duration;

pub fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent("bluesky-techbot")
        .timeout(Duration::from_secs(30))
        .build()?)
}
