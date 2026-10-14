use crate::judge::{compose, is_japanese};
use serde_json::{Value, json};

pub const TEXT_LIMIT: usize = 300;

pub struct Draft {
    pub text: String,
    pub link: String,
    pub card_title: String,
    pub card_description: String,
    pub card_image: Option<String>,
}

pub fn draft(
    post_title: &str,
    post_body: &str,
    post_point: &str,
    url: &str,
    title: &str,
    card_description: &str,
    card_image: Option<&str>,
) -> Draft {
    let head = if is_japanese(title) {
        title
    } else {
        post_title
    };
    Draft {
        text: compose(head, post_body, post_point),
        link: url.to_string(),
        card_title: title.to_string(),
        card_description: card_description.to_string(),
        card_image: card_image
            .map(|image| image.trim().to_string())
            .filter(|image| !image.is_empty()),
    }
}

pub fn record(draft: &Draft, created_at: &str) -> Value {
    json!({
        "$type": "app.bsky.feed.post",
        "text": draft.text,
        "createdAt": created_at,
        "langs": ["ja"],
        "embed": {
            "$type": "app.bsky.embed.external",
            "external": {
                "uri": draft.link,
                "title": draft.card_title,
                "description": draft.card_description
            }
        }
    })
}

pub fn too_long(draft: &Draft) -> bool {
    draft.text.chars().count() > TEXT_LIMIT
}

pub fn empty(draft: &Draft) -> bool {
    draft.text.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 画像の場所が空のときは、画像なしとして扱うこと
    #[test]
    fn treats_empty_image_as_none() {
        let empty = draft(
            "題名",
            "要約",
            "要点",
            "https://example.com/a",
            "元",
            "紹介",
            Some(""),
        );
        let blank = draft(
            "題名",
            "要約",
            "要点",
            "https://example.com/a",
            "元",
            "紹介",
            Some("  "),
        );
        let given = draft(
            "題名",
            "要約",
            "要点",
            "https://example.com/a",
            "元",
            "紹介",
            Some("https://example.com/a.png"),
        );

        assert_eq!(empty.card_image, None);
        assert_eq!(blank.card_image, None);
        assert_eq!(
            given.card_image.as_deref(),
            Some("https://example.com/a.png")
        );
    }

    /// 投稿レコードは、本文・作成日時・リンクカードを持つこと
    #[test]
    fn builds_record() {
        let draft = draft(
            "題名",
            "要約",
            "要点",
            "https://example.com/a",
            "元の題名",
            "紹介文",
            None,
        );
        let record = record(&draft, "2026-10-13T00:00:00Z");

        assert_eq!(record["$type"], "app.bsky.feed.post");
        assert_eq!(record["text"], "元の題名\n要約\n\n要点");
        assert_eq!(record["createdAt"], "2026-10-13T00:00:00Z");
        assert_eq!(record["embed"]["external"]["uri"], "https://example.com/a");
        assert_eq!(record["embed"]["external"]["title"], "元の題名");
        assert_eq!(record["embed"]["external"]["description"], "紹介文");
    }

    /// 本文は日本語として投稿すること
    #[test]
    fn marks_language_as_japanese() {
        let draft = draft(
            "題名",
            "",
            "",
            "https://example.com/a",
            "元の題名",
            "",
            None,
        );
        let record = record(&draft, "2026-10-13T00:00:00Z");

        assert_eq!(record["langs"][0], "ja");
    }

    /// 英語の記事では、訳した題名を本文の先頭に置くこと
    #[test]
    fn keeps_title_for_english_article() {
        let draft = draft(
            "電球型コンピュータ（英語）",
            "要約",
            "要点",
            "https://example.com/a",
            "The Lightbulb Computer",
            "",
            None,
        );

        assert_eq!(draft.text, "電球型コンピュータ（英語）\n要約\n\n要点");
    }

    /// 日本語の記事では、元ページの題名をそのまま本文の先頭に置くこと
    #[test]
    fn keeps_original_title_for_japanese_article() {
        let draft = draft(
            "AIが書き換えた題名",
            "要約",
            "要点",
            "https://example.com/a",
            "元の題名",
            "",
            None,
        );

        assert_eq!(draft.text, "元の題名\n要約\n\n要点");
    }

    /// 本文が空の投稿を、空と判定できること
    #[test]
    fn detects_empty_text() {
        let blank = draft("", "", "", "https://example.com/a", "", "", None);
        assert!(empty(&blank));

        let filled = draft(
            "訳題",
            "要約",
            "",
            "https://example.com/a",
            "題名",
            "",
            None,
        );
        assert!(!empty(&filled));
    }

    /// 本文が300字を超えるかどうかを判定できること
    #[test]
    fn detects_too_long_text() {
        let short = draft("あ", "い", "う", "https://example.com/a", "題名", "", None);
        assert!(!too_long(&short));

        let long_body = "あ".repeat(301);
        let long = draft(
            "題名",
            &long_body,
            "",
            "https://example.com/a",
            "題名",
            "",
            None,
        );
        assert!(too_long(&long));
    }
}
