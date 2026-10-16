use crate::judge::{compose, is_japanese};
use serde_json::{Value, json};

pub const TEXT_LIMIT: usize = 300;
pub const TEXT_MIN: usize = 40;

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
    Draft {
        text: fit_text(head(title, post_title), post_body, post_point),
        link: url.to_string(),
        card_title: title.to_string(),
        card_description: card_description.to_string(),
        card_image: card_image
            .map(|image| image.trim().to_string())
            .filter(|image| !image.is_empty()),
    }
}

pub fn head<'a>(title: &'a str, post_title: &'a str) -> &'a str {
    if is_japanese(title) {
        title
    } else {
        post_title
    }
}

pub fn fit_text(head: &str, body: &str, point: &str) -> String {
    for (body, point) in [(body, point), ("", point), ("", "")] {
        let text = compose(head, body, point);
        if text.chars().count() <= TEXT_LIMIT {
            return text;
        }
    }
    head.chars().take(TEXT_LIMIT - 1).chain(['…']).collect()
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

pub fn too_short(draft: &Draft) -> bool {
    draft.text.chars().count() < TEXT_MIN
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

    /// 本文が40字に満たない投稿を、短すぎると判定できること
    #[test]
    fn detects_too_short_text() {
        let blank = draft("", "", "", "https://example.com/a", "", "", None);
        assert!(too_short(&blank));

        let short = draft(
            "訳題",
            "要約",
            "",
            "https://example.com/a",
            "題名",
            "",
            None,
        );
        assert!(too_short(&short));

        let filled = draft(
            "訳題",
            "記事に書かれていることを伝える要約の一文です。",
            "他の場面にも当てはまる形まで言い切った要点の一文です。",
            "https://example.com/a",
            "題名",
            "",
            None,
        );
        assert!(!too_short(&filled));
    }

    /// 本文が300字を超えるかどうかを判定できること
    #[test]
    fn detects_too_long_text() {
        let short = draft("あ", "い", "う", "https://example.com/a", "題名", "", None);
        assert!(!too_long(&short));

        let long = Draft {
            text: "あ".repeat(301),
            link: "https://example.com/a".into(),
            card_title: "題名".into(),
            card_description: String::new(),
            card_image: None,
        };
        assert!(too_long(&long));
    }

    /// 上限に収まっているときは、要約も要点も残すこと
    #[test]
    fn keeps_all_lines_within_limit() {
        assert_eq!(fit_text("題名", "要約", "要点"), "題名\n要約\n\n要点");
    }

    /// 上限を超えるときは、要約を外して収めること
    #[test]
    fn drops_body_to_fit() {
        let head = "あ".repeat(200);
        let text = fit_text(&head, &"い".repeat(60), &"う".repeat(60));

        assert_eq!(text, format!("{head}\n\n{}", "う".repeat(60)));
        assert!(text.chars().count() <= TEXT_LIMIT);
    }

    /// 要約を外しても超えるときは、要点も外して題名だけにすること
    #[test]
    fn drops_point_to_fit() {
        let head = "あ".repeat(280);
        let text = fit_text(&head, "要約", &"う".repeat(60));

        assert_eq!(text, head);
    }

    /// 題名だけで上限を超えるときは、題名を切り詰めて収めること
    #[test]
    fn trims_long_head() {
        let text = fit_text(&"あ".repeat(400), "要約", "要点");

        assert_eq!(text.chars().count(), TEXT_LIMIT);
        assert!(text.ends_with('…'));
    }

    /// 投稿する1行目は、題名が日本語ならその題名、そうでなければ訳した題名にすること
    #[test]
    fn picks_head_by_language() {
        assert_eq!(head("日本語の題名", "訳した題名"), "日本語の題名");
        assert_eq!(head("English title", "訳した題名"), "訳した題名");
    }
}
