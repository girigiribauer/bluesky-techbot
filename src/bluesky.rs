use crate::config::Config;
use anyhow::{Context, Result, anyhow};
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

pub fn app_password(config: &Config) -> Result<String> {
    if let Ok(password) = std::env::var("BLUESKY_APP_PASSWORD")
        && !password.trim().is_empty()
    {
        return Ok(password.trim().to_string());
    }
    let path = &config.bluesky.password_file;
    let password = std::fs::read_to_string(path)
        .with_context(|| format!("アプリパスワードが読めません: {path}"))?;
    Ok(password.trim().to_string())
}

pub const BLOB_LIMIT: usize = 900_000;

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
    if let Some(url) = thumb_source
        && let Some(blob) = upload_thumb(client, config, &session, url).await
    {
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
) -> Option<Value> {
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = response.bytes().await.ok()?.to_vec();
    let (bytes, mime) = shrink(bytes)?;

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
        .ok()?;
    if !uploaded.status().is_success() {
        return None;
    }
    let body: Value = uploaded.json().await.ok()?;
    body.get("blob").cloned()
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
