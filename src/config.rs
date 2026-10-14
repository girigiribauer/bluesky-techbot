use anyhow::Result;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct Config {
    pub sources: Sources,
    pub hackernews: HackerNews,
    pub collect: Collect,
    pub limits: Limits,
    pub judge: Judge,
    pub bluesky: Bluesky,
    pub posting: Posting,
    pub web: Web,
    pub db: Db,
}

#[derive(Clone, Deserialize)]
pub struct Sources {
    pub zenn: String,
    pub hatena: String,
}

#[derive(Clone, Deserialize)]
pub struct HackerNews {
    pub min_points: i64,
    pub window_hours: i64,
}

#[derive(Clone, Deserialize)]
pub struct Collect {
    pub interval_minutes: u64,
    pub fetch_batch: i64,
    pub fetch_delay_seconds: u64,
    pub max_attempts: i64,
}

#[derive(Clone, Deserialize)]
pub struct Limits {
    pub slides: usize,
    pub body_chars: usize,
    pub min_body_chars: usize,
}

#[derive(Clone, Deserialize)]
pub struct Posting {
    pub interval_minutes: u64,
    pub per_batch: usize,
    pub gap_seconds: u64,
    pub daily_limit: i64,
    pub expire_hours: i64,
}

#[derive(Clone, Deserialize)]
pub struct Bluesky {
    pub handle: String,
    pub service: String,
    pub password_file: String,
}

#[derive(Clone, Deserialize)]
pub struct Judge {
    pub model: String,
    pub key_file: String,
    pub batch: i64,
}

#[derive(Clone, Deserialize)]
pub struct Web {
    pub port: u16,
}

#[derive(Clone, Deserialize)]
pub struct Db {
    pub path: String,
}

pub fn load(path: &str) -> Result<Config> {
    let text = std::fs::read_to_string(path)?;
    let mut config: Config = toml::from_str(&text)?;
    if let Ok(db_path) = std::env::var("TECHBOT_DB_PATH")
        && !db_path.trim().is_empty()
    {
        config.db.path = db_path.trim().to_string();
    }
    Ok(config)
}
