use crate::config::Config;
use crate::extract::Transient;
use anyhow::{Result, anyhow};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Session {
    #[serde(rename = "accessJwt")]
    access_jwt: String,
    did: String,
}

#[derive(Deserialize)]
struct Created {
    uri: String,
}

pub fn app_password() -> Result<String> {
    let password = std::env::var("BLUESKY_APP_PASSWORD").unwrap_or_default();
    if password.trim().is_empty() {
        return Err(anyhow!(
            "環境変数 BLUESKY_APP_PASSWORD が設定されていません"
        ));
    }
    Ok(password.trim().to_string())
}

pub const BLOB_LIMIT: usize = 900_000;

const THUMB_TRIES: usize = 3;
const THUMB_RETRY_SECONDS: u64 = 2;

#[derive(Debug)]
pub struct ThumbUnusable(pub String);

impl std::fmt::Display for ThumbUnusable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl std::error::Error for ThumbUnusable {}

pub fn is_thumb_unusable(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| cause.is::<ThumbUnusable>())
}

#[derive(Deserialize)]
struct Records {
    records: Vec<Record>,
    cursor: Option<String>,
}

#[derive(Deserialize)]
struct Record {
    value: Value,
}

pub async fn posted_links(
    client: &reqwest::Client,
    config: &Config,
) -> Result<std::collections::HashSet<String>> {
    let did = resolve_handle(client, config).await?;
    let mut links = std::collections::HashSet::new();
    let mut cursor: Option<String> = None;

    loop {
        let mut request = client
            .get(format!(
                "{}/xrpc/com.atproto.repo.listRecords",
                config.bluesky.service
            ))
            .query(&[
                ("repo", did.as_str()),
                ("collection", "app.bsky.feed.post"),
                ("limit", "100"),
            ]);
        if let Some(value) = &cursor {
            request = request.query(&[("cursor", value.as_str())]);
        }

        let page: Records = request.send().await?.error_for_status()?.json().await?;
        for record in &page.records {
            if let Some(uri) = record
                .value
                .pointer("/embed/external/uri")
                .and_then(|value| value.as_str())
            {
                links.insert(uri.to_string());
            }
        }

        cursor = page.cursor;
        if cursor.is_none() || page.records.is_empty() {
            break;
        }
    }
    Ok(links)
}

async fn resolve_handle(client: &reqwest::Client, config: &Config) -> Result<String> {
    #[derive(Deserialize)]
    struct Resolved {
        did: String,
    }

    let resolved: Resolved = client
        .get(format!(
            "{}/xrpc/com.atproto.identity.resolveHandle",
            config.bluesky.service
        ))
        .query(&[("handle", config.bluesky.handle.as_str())])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(resolved.did)
}

pub async fn post(
    client: &reqwest::Client,
    config: &Config,
    password: &str,
    record: &Value,
    thumb_source: Option<&str>,
) -> Result<String> {
    let session = login(client, config, password).await?;

    let mut record = record.clone();
    if let Some(url) = thumb_source {
        let mut attempt = 1;
        let blob = loop {
            match upload_thumb(client, config, &session, url).await {
                Ok(blob) => break blob,
                Err(error) if attempt < THUMB_TRIES && crate::extract::is_transient(&error) => {
                    attempt += 1;
                    tokio::time::sleep(std::time::Duration::from_secs(THUMB_RETRY_SECONDS)).await;
                }
                Err(error) => {
                    return Err(error.context("リンクカードの画像を添付できません"));
                }
            }
        };
        record["embed"]["external"]["thumb"] = blob;
    }

    let response = client
        .post(format!(
            "{}/xrpc/com.atproto.repo.createRecord",
            config.bluesky.service
        ))
        .bearer_auth(&session.access_jwt)
        .json(&json!({
            "repo": session.did,
            "collection": "app.bsky.feed.post",
            "record": record
        }))
        .send()
        .await?;

    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        return Err(anyhow!("投稿に失敗しました（{status}）: {text}"));
    }

    let created: Created = serde_json::from_str(&text)?;
    Ok(created.uri)
}

async fn upload_thumb(
    client: &reqwest::Client,
    config: &Config,
    session: &Session,
    url: &str,
) -> Result<Value> {
    let response = crate::extract::get(client, url).await?;
    if !response.status().is_success() {
        return Err(anyhow::Error::new(ThumbUnusable(format!(
            "画像を取得できません（{}）: {url}",
            response.status()
        ))));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| {
            anyhow::Error::new(Transient(format!("画像の受け取りが切れました: {error}")))
        })?
        .to_vec();
    let (bytes, mime) = shrink(bytes).ok_or_else(|| {
        anyhow::Error::new(ThumbUnusable(format!("画像を使える形にできません: {url}")))
    })?;

    let uploaded = client
        .post(format!(
            "{}/xrpc/com.atproto.repo.uploadBlob",
            config.bluesky.service
        ))
        .bearer_auth(&session.access_jwt)
        .header("content-type", mime)
        .body(bytes)
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() || error.is_connect() || error.is_request() {
                anyhow::Error::new(Transient(format!("画像を上げられません: {error}")))
            } else {
                anyhow::Error::new(error)
            }
        })?;
    let status = uploaded.status();
    if !status.is_success() {
        let text = uploaded.text().await.unwrap_or_default();
        return Err(anyhow!("画像の受け取りを断られました（{status}）: {text}"));
    }
    let body: Value = uploaded.json().await?;
    body.get("blob")
        .cloned()
        .ok_or_else(|| anyhow!("画像の返事に blob がありません"))
}

fn shrink(bytes: Vec<u8>) -> Option<(Vec<u8>, &'static str)> {
    if bytes.len() <= BLOB_LIMIT {
        let mime = match image::guess_format(&bytes).ok()? {
            image::ImageFormat::Png => "image/png",
            image::ImageFormat::Jpeg => "image/jpeg",
            image::ImageFormat::WebP => "image/webp",
            image::ImageFormat::Gif => "image/gif",
            _ => return None,
        };
        return Some((bytes, mime));
    }

    let decoded = image::load_from_memory(&bytes).ok()?;
    let resized = decoded.resize(1200, 1200, image::imageops::FilterType::Triangle);
    let mut encoded = std::io::Cursor::new(Vec::new());
    resized
        .to_rgb8()
        .write_to(&mut encoded, image::ImageFormat::Jpeg)
        .ok()?;
    let encoded = encoded.into_inner();
    if encoded.len() > BLOB_LIMIT {
        return None;
    }
    Some((encoded, "image/jpeg"))
}

async fn login(client: &reqwest::Client, config: &Config, password: &str) -> Result<Session> {
    let response = client
        .post(format!(
            "{}/xrpc/com.atproto.server.createSession",
            config.bluesky.service
        ))
        .json(&json!({
            "identifier": config.bluesky.handle,
            "password": password
        }))
        .send()
        .await?;

    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        return Err(anyhow!("ログインに失敗しました（{status}）"));
    }
    Ok(serde_json::from_str(&text)?)
}

pub fn web_url(handle: &str, uri: &str) -> String {
    match uri.rsplit('/').next() {
        Some(key) => format!("https://bsky.app/profile/{handle}/post/{key}"),
        None => uri.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 画像が使えないと分かった誤りは、通信の一時的な失敗と区別できること
    #[test]
    fn separates_unusable_thumb_from_transient() {
        let unusable = anyhow::Error::new(ThumbUnusable("画像を取得できません（404）".into()));
        let transient = anyhow::Error::new(Transient("画像の受け取りが切れました".into()));

        assert!(is_thumb_unusable(&unusable));
        assert!(!crate::extract::is_transient(&unusable));
        assert!(!is_thumb_unusable(&transient));
        assert!(crate::extract::is_transient(&transient));
    }

    /// 投稿後に返る at:// 形式のURIから、ブラウザで開けるURLを組み立てること
    #[test]
    fn builds_web_url() {
        assert_eq!(
            web_url(
                "tech-invariants.bsky.social",
                "at://did:plc:abc/app.bsky.feed.post/3kxyz"
            ),
            "https://bsky.app/profile/tech-invariants.bsky.social/post/3kxyz"
        );
    }
}
