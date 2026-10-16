mod article;
mod speakerdeck;

use crate::config::Limits;
use anyhow::Result;
use std::fmt;

#[derive(Debug)]
pub struct Transient(pub String);

impl fmt::Display for Transient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl std::error::Error for Transient {}

pub fn is_transient(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| cause.is::<Transient>())
}

pub async fn get_text(client: &reqwest::Client, url: &str) -> Result<String> {
    let response = get(client, url).await?;
    let header_charset = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(charset_from_content_type);
    let bytes = response.bytes().await?;

    let charset = header_charset.or_else(|| sniff_charset(&bytes));
    let encoding = charset
        .as_deref()
        .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    let (text, _, _) = encoding.decode(&bytes);
    Ok(text.into_owned())
}

fn charset_from_content_type(value: &str) -> Option<String> {
    let pattern = regex::Regex::new(r#"(?i)charset\s*=\s*["']?([\w-]+)"#).ok()?;
    Some(pattern.captures(value)?.get(1)?.as_str().to_lowercase())
}

fn sniff_charset(bytes: &[u8]) -> Option<String> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let meta = regex::Regex::new(r#"(?i)<meta[^>]*charset\s*=\s*["']?\s*([\w-]+)"#).ok()?;
    Some(meta.captures(&head)?.get(1)?.as_str().to_lowercase())
}

pub async fn get(client: &reqwest::Client, url: &str) -> Result<reqwest::Response> {
    let response = client.get(url).send().await.map_err(|error| {
        if error.is_timeout() || error.is_connect() || error.is_request() {
            anyhow::Error::new(Transient(format!("接続できません: {error}")))
        } else {
            anyhow::Error::new(error)
        }
    })?;

    let status = response.status();
    if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(anyhow::Error::new(Transient(format!(
            "相手のサーバーが{status}を返しました"
        ))));
    }
    Ok(response.error_for_status()?)
}

#[derive(Default)]
pub struct Card {
    pub title: Option<String>,
    pub description: String,
    pub image: Option<String>,
}

pub enum Content {
    Text {
        text: String,
        truncated: bool,
        card: Card,
    },
    Slides {
        urls: Vec<String>,
        truncated: bool,
        card: Card,
    },
}

pub fn card(html: &str, url: &str) -> Card {
    let Ok(readability) = dom_smoothie::Readability::new(html, Some(url), None) else {
        return Card::default();
    };
    let mut card = from_metadata(
        readability.get_article_metadata(readability.parse_json_ld()),
        url,
    );
    if card.image.is_none() {
        card.image = meta_image(html).and_then(|image| crate::url::absolutize(url, &image));
    }
    card
}

fn meta_image(html: &str) -> Option<String> {
    let tag = regex::Regex::new(r"(?is)<meta\b[^>]*>").ok()?;
    let attribute =
        regex::Regex::new(r#"(?is)([a-z:_-]+)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#).ok()?;
    let mut twitter = None;
    for found in tag.find_iter(html) {
        let mut key = String::new();
        let mut content = String::new();
        for captures in attribute.captures_iter(found.as_str()) {
            let value = captures
                .get(2)
                .or_else(|| captures.get(3))
                .or_else(|| captures.get(4))
                .map_or("", |value| value.as_str())
                .trim();
            match captures[1].to_ascii_lowercase().as_str() {
                "name" | "property" => key = value.to_ascii_lowercase(),
                "content" => content = value.replace("&amp;", "&"),
                _ => {}
            }
        }
        if content.is_empty() {
            continue;
        }
        match key.as_str() {
            "og:image" => return Some(content),
            "twitter:image" if twitter.is_none() => twitter = Some(content),
            _ => {}
        }
    }
    twitter
}

pub fn from_metadata(metadata: dom_smoothie::Metadata, url: &str) -> Card {
    Card {
        title: Some(metadata.title).filter(|title| !title.trim().is_empty()),
        description: metadata.excerpt.unwrap_or_default(),
        image: metadata
            .image
            .and_then(|image| crate::url::absolutize(url, &image)),
    }
}

pub async fn extract(client: &reqwest::Client, url: &str, limits: &Limits) -> Result<Content> {
    if url.contains("speakerdeck.com/") {
        speakerdeck::extract(client, url, limits).await
    } else {
        article::extract(client, url, limits).await
    }
}

pub fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 応答ヘッダの content-type から文字コードを読み取ること
    #[test]
    fn reads_charset_from_header() {
        assert_eq!(
            charset_from_content_type("text/html; charset=Shift_JIS").as_deref(),
            Some("shift_jis")
        );
        assert!(charset_from_content_type("text/html").is_none());
    }

    /// ヘッダに無ければ、HTML の meta から文字コードを読み取ること
    #[test]
    fn sniffs_charset_from_html() {
        let old = br#"<meta http-equiv="content-type" content="text/html;charset=shift_jis">"#;
        assert_eq!(sniff_charset(old).as_deref(), Some("shift_jis"));

        let modern = br#"<meta charset="utf-8">"#;
        assert_eq!(sniff_charset(modern).as_deref(), Some("utf-8"));

        assert!(sniff_charset(b"<html><body>no meta</body></html>").is_none());
    }

    /// og:description と og:image を取り出すこと
    #[test]
    fn reads_open_graph_tags() {
        let html = r#"<meta property="og:description" content="記事の紹介文">
<meta property="og:image" content="https://example.com/a.jpg">"#;
        let card = card(html, "https://example.com/a");

        assert_eq!(card.description, "記事の紹介文");
        assert_eq!(card.image.as_deref(), Some("https://example.com/a.jpg"));
    }

    /// 属性が引用符で囲まれていなくても、og:description と og:image を取り出すこと
    #[test]
    fn reads_unquoted_attributes() {
        let html = r#"<meta content=記事の紹介文です。 property=og:description>
<meta content=https://example.com/a.png property=og:image>"#;
        let card = card(html, "https://example.com/a");

        assert_eq!(card.description, "記事の紹介文です。");
        assert_eq!(card.image.as_deref(), Some("https://example.com/a.png"));
    }

    /// 似た名前のタグ（og:image:width）を、og:image と取り違えないこと
    #[test]
    fn keeps_similar_tag_names_apart() {
        let html = r#"<meta content=1200 property=og:image:width>
<meta content=https://example.com/a.png property=og:image>"#;

        assert_eq!(
            card(html, "https://example.com/a").image.as_deref(),
            Some("https://example.com/a.png")
        );
    }

    /// og: が無ければ twitter: や description を使うこと
    #[test]
    fn falls_back_to_other_tags() {
        let html = r#"<meta name="twitter:description" content="予備の紹介文">"#;
        assert_eq!(
            card(html, "https://example.com/a").description,
            "予備の紹介文"
        );
    }

    /// 該当するタグが無ければ、説明文は空で画像は無しになること
    #[test]
    fn returns_empty_without_tags() {
        let card = card(
            "<html><body>本文だけ</body></html>",
            "https://example.com/a",
        );

        assert!(card.description.is_empty());
        assert!(card.image.is_none());
    }

    /// 題名は og:title を優先し、無ければ title タグを使うこと
    #[test]
    fn reads_title_in_order() {
        let both = r#"<title>タイトルタグ</title><meta property="og:title" content="OGの題名">"#;
        assert_eq!(
            card(both, "https://example.com/a").title.as_deref(),
            Some("OGの題名")
        );

        let only_tag = "<title>タイトルタグ</title>";
        assert_eq!(
            card(only_tag, "https://example.com/a").title.as_deref(),
            Some("タイトルタグ")
        );
    }

    /// 画像の相対パスを、元のURLを基準に絶対化すること
    #[test]
    fn absolutizes_image() {
        let html = r#"<meta property="og:image" content="/ogp.png">"#;
        assert_eq!(
            card(html, "https://example.com/blog/entry")
                .image
                .as_deref(),
            Some("https://example.com/ogp.png")
        );
    }

    /// property ではなく name で書かれた og:image も取り出すこと
    #[test]
    fn reads_image_from_name_attribute() {
        let html = r#"<meta name="og:image" content="https://example.com/a.jpg">"#;

        assert_eq!(
            card(html, "https://example.com/a").image.as_deref(),
            Some("https://example.com/a.jpg")
        );
    }

    /// content が先に書かれていても、name の og:image を取り出すこと
    #[test]
    fn reads_image_whatever_the_attribute_order() {
        let html = r#"<meta content="https://example.com/a.jpg" name="og:image">"#;

        assert_eq!(
            card(html, "https://example.com/a").image.as_deref(),
            Some("https://example.com/a.jpg")
        );
    }

    /// name で書かれた og:image が無ければ、name の twitter:image を使うこと
    #[test]
    fn falls_back_to_twitter_image() {
        let html = r#"<meta name="twitter:image" content="https://example.com/t.jpg">"#;

        assert_eq!(
            card(html, "https://example.com/a").image.as_deref(),
            Some("https://example.com/t.jpg")
        );
    }

    /// name で書かれた og:image:width を、og:image と取り違えないこと
    #[test]
    fn keeps_similar_name_attributes_apart() {
        let html = r#"<meta name="og:image:width" content="1200">
<meta name="og:image" content="https://example.com/a.jpg">"#;

        assert_eq!(
            card(html, "https://example.com/a").image.as_deref(),
            Some("https://example.com/a.jpg")
        );
    }

    /// 画像のURLに書かれた &amp; を & に戻すこと
    #[test]
    fn unescapes_image_url() {
        let html = r#"<meta name="og:image" content="https://example.com/a.png?w=1200&amp;h=630">"#;

        assert_eq!(
            card(html, "https://example.com/a").image.as_deref(),
            Some("https://example.com/a.png?w=1200&h=630")
        );
    }

    /// 実体参照を元の文字に戻すこと
    #[test]
    fn unescapes_entities() {
        let html = r#"<meta property="og:description" content="A &amp; B">"#;
        assert_eq!(card(html, "https://example.com/a").description, "A & B");
    }

    /// 数字で書かれた文字（&#34; や &#x27;）も、元の文字に戻すこと
    #[test]
    fn unescapes_numeric_entities() {
        let html = r#"<meta property="og:description" content="I&#x27;ve said &#34;hello&#34;">"#;

        assert_eq!(
            card(html, "https://example.com/a").description,
            "I've said \"hello\""
        );
    }
}
