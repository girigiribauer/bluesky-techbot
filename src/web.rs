use crate::db;
use anyhow::Result;
use axum::Router;
use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;
use serde::Deserialize;
use sqlx::SqlitePool;

pub async fn serve(port: u16, pool: SqlitePool) -> Result<()> {
    let app = Router::new()
        .route("/", get(index))
        .route("/record", get(record))
        .with_state(pool);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    log!("http://127.0.0.1:{port}");
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Deserialize)]
struct View {
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    q: Option<String>,
}

async fn index(State(pool): State<SqlitePool>, Query(view): Query<View>) -> Html<String> {
    let state = view.state.unwrap_or_else(|| "pass".to_string());
    let search = view.q.unwrap_or_default();
    match render(&pool, &state, search.trim()).await {
        Ok(html) => Html(html),
        Err(error) => Html(format!("<pre>{}</pre>", escape(&format!("{error:#}")))),
    }
}

#[derive(Deserialize)]
struct Target {
    id: i64,
}

async fn record(State(pool): State<SqlitePool>, Query(target): Query<Target>) -> Html<String> {
    let Ok(Some(queued)) = db::queued(&pool, target.id).await else {
        return Html("<p>見つかりません</p>".to_string());
    };
    let draft = crate::post::draft(
        &queued.post_title,
        &queued.post_body,
        &queued.post_point,
        &queued.url,
        queued.card_title.as_deref().unwrap_or(&queued.title),
        &queued.card_description,
        queued.card_image.as_deref(),
    );
    let record = crate::post::record(&draft, &crate::clock::now_text());
    let json = serde_json::to_string_pretty(&record).unwrap_or_default();
    Html(format!(
        "<!DOCTYPE html><html lang=\"ja\"><head><meta charset=\"utf-8\"><title>投稿レコード</title>
<style>body {{ font-family: system-ui, sans-serif; margin: 2rem; }} pre {{ background: #f6f7f9; padding: 1rem; border-radius: 8px; overflow: auto; }}</style>
</head><body><p><a href=\"/?state=pass\">戻る</a></p><pre>{}</pre></body></html>",
        escape(&json)
    ))
}

const STYLE: &str = "
:root { color-scheme: light dark; }
body { font-family: system-ui, sans-serif; margin: 0; background: #f6f7f9; color: #111; }
header { position: sticky; top: 0; z-index: 10; background: #fff; border-bottom: 1px solid #e3e6ea; padding: 1rem 1.5rem; }
header .counts span { margin-right: 1rem; color: #777; font-size: 0.8rem; }
.tabs { display: flex; gap: 0.3rem; margin-top: 0.6rem; }
.tab { padding: 0.35rem 0.9rem; border-radius: 999px; text-decoration: none; color: #444; font-size: 0.88rem; background: #eef0f3; }
.tab b { font-weight: 600; color: #666; }
.tab.current { background: #15417e; color: #fff; }
.tab.current b { color: #cfe0ff; }
main { max-width: 56rem; margin: 0 auto; padding: 1.5rem; }
.item { background: #fff; border: 1px solid #e3e6ea; border-radius: 10px; padding: 0.9rem 1.1rem; margin-bottom: 0.8rem; }
.item.pass { border-left: 4px solid #3f9f5f; }
.item.drop { border-left: 4px solid #cfd4da; opacity: 0.72; }
.item.failed { border-left: 4px solid #d9534f; }
.meta a { color: #15417e; }
.meta { color: #777; font-size: 0.78rem; display: flex; gap: 0.6rem; flex-wrap: wrap; margin-bottom: 0.35rem; }
.title { font-size: 0.98rem; font-weight: 600; line-height: 1.45; }
.search { display: flex; gap: 0.4rem; margin: 0.6rem 0 0.1rem; }
.tabs { margin-top: 0.6rem; }
.search input { flex: 1; max-width: 22rem; padding: 0.35rem 0.7rem; border: 1px solid #d4dbe2; border-radius: 999px; font-size: 0.85rem; background: #fff; color: #111; }
.search button { padding: 0.35rem 0.9rem; border: 0; border-radius: 999px; background: #15417e; color: #fff; font-size: 0.85rem; cursor: pointer; }
.search .clear { padding: 0.35rem 0.9rem; border-radius: 999px; background: #eef0f3; color: #444; text-decoration: none; font-size: 0.85rem; display: inline-flex; align-items: center; }
.shown { color: #777; font-size: 0.78rem; margin-bottom: 0.7rem; }
.tags { display: flex; gap: 0.35rem; flex-wrap: wrap; margin-top: 0.45rem; }
.tag { font-size: 0.72rem; padding: 0.12rem 0.5rem 0.14rem; border-radius: 999px; background: #eef0f3; color: #39404a; }
.tag i { font-style: normal; opacity: 0.6; margin-right: 0.35rem; font-size: 0.68rem; }
.tag.basis { background: #e5ecf9; color: #1c4382; }
.tag.depth.center { background: #dfe8f5; color: #1f3f6e; }
.tag.depth.part { background: #eceef1; color: #5a6270; }
.tag.depth.hint { background: #f3f4f6; color: #8a94a0; }
.tag.field { background: #e4f1e8; color: #246b3a; }
.tag.form { background: #efe8f6; color: #5b3d86; }
.title a { color: #15417e; text-decoration: none; }
.title a:hover { text-decoration: underline; }
.badge { display: inline-block; font-size: 0.72rem; font-weight: 600; padding: 0.1rem 0.45rem; border-radius: 999px; margin-left: 0.4rem; vertical-align: middle; }
.badge.pass { background: #e4f3e8; color: #246b3a; }
.badge.drop { background: #eceef1; color: #5a6270; }
.badge.failed { background: #fbe6e5; color: #9b2d2a; }
.reason { color: #555; font-size: 0.85rem; margin-top: 0.4rem; line-height: 1.6; }
.post { margin-top: 0.8rem; width: 520px; max-width: 100%; border: 1px solid #d9e2ea; border-radius: 12px; padding: 0.8rem 0.9rem 0.7rem; background: #fff; }
.post .label { font-size: 0.7rem; color: #8a94a0; margin-bottom: 0.45rem; letter-spacing: 0.08em; }
.post .body { font-size: 15px; line-height: 20px; white-space: pre-wrap; letter-spacing: 0.01em; }
.post .warn { margin-top: 0.4rem; color: #b03030; font-size: 0.75rem; }
.post .count { margin-top: 0.4rem; color: #999; font-size: 0.72rem; }
.post .record { display: inline-block; margin-top: 0.3rem; font-size: 0.72rem; color: #15417e; }
.post .card { display: block; margin-top: 0.6rem; border: 1px solid #d4dbe2; border-radius: 10px; padding: 0.6rem 0.75rem; text-decoration: none; }
.post .card-title { font-size: 14px; line-height: 18px; color: #0b1620; }
.post .card-desc { font-size: 13px; line-height: 18px; color: #42576b; margin-top: 0.25rem; }
.post .card-domain { font-size: 13px; line-height: 18px; color: #6b7785; margin-top: 0.2rem; }
.post .card:hover { background: #f4f7fa; }
";

async fn render(pool: &SqlitePool, state: &str, search: &str) -> Result<String> {
    let counts = db::count_by_source(pool).await?;
    let found = (!search.is_empty()).then_some(search);
    let (total, passed, dropped, failed, posted) = db::count_states(pool, found).await?;
    let articles = db::recent(pool, state, found, 300).await?;

    let mut summary = String::new();
    for (source, count) in &counts {
        summary.push_str(&format!("<span>{} {}</span>", escape(source), count));
    }

    let tabs = [
        ("pass", "投稿待ち", passed),
        ("posted", "投稿済み", posted),
        ("drop", "落とす", dropped),
        ("failed", "失敗", failed),
        ("all", "全部", total),
    ]
    .iter()
    .map(|(key, label, count)| {
        let current = if *key == state { " current" } else { "" };
        let keep = if search.is_empty() {
            String::new()
        } else {
            format!("&q={}", urlencode(search))
        };
        format!("<a class=\"tab{current}\" href=\"/?state={key}{keep}\">{label} <b>{count}</b></a>")
    })
    .collect::<String>();

    let count = tab_count(state, total, passed, dropped, failed, posted);
    let phrase = shown_phrase(state, found.is_some());
    let head = match found {
        Some(word) => format!("「{}」で{phrase}", escape(word)),
        None => phrase.to_string(),
    };
    let shown = if articles.is_empty() {
        match found {
            Some(word) => format!(
                "<div class=\"shown\">「{}」に当てはまる記事はありません</div>",
                escape(word)
            ),
            None => format!("<div class=\"shown\">{phrase}記事はありません</div>"),
        }
    } else if (articles.len() as i64) < count {
        format!(
            "<div class=\"shown\">{head}{count}件のうち、新しい{}件を表示しています</div>",
            articles.len()
        )
    } else {
        format!(
            "<div class=\"shown\">{head}{}件を表示しています</div>",
            articles.len()
        )
    };

    let clear = match found {
        Some(_) => format!(
            "<a class=\"clear\" href=\"/?state={}\">クリア</a>",
            escape(state)
        ),
        None => String::new(),
    };
    let form = format!(
        "<form class=\"search\" method=\"get\" action=\"/\">
<input type=\"hidden\" name=\"state\" value=\"{}\">
<input type=\"search\" name=\"q\" value=\"{}\" placeholder=\"題名・URL・投稿文から探す\">
<button type=\"submit\">探す</button>{clear}
</form>",
        escape(state),
        escape(search)
    );

    let mut items = String::new();
    for article in &articles {
        items.push_str(&item(article));
    }

    Ok(format!(
        "<!DOCTYPE html>
<html lang=\"ja\">
<head>
<meta charset=\"utf-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
<title>techbot</title>
<style>{STYLE}</style>
</head>
<body>
<header><div class=\"counts\">{summary}</div>{form}<nav class=\"tabs\">{tabs}</nav></header>
<main>{shown}{items}</main>
</body>
</html>"
    ))
}

fn tab_count(state: &str, total: i64, passed: i64, dropped: i64, failed: i64, posted: i64) -> i64 {
    match state {
        "posted" => posted,
        "drop" => dropped,
        "failed" => failed,
        "all" => total,
        _ => passed,
    }
}

fn shown_phrase(state: &str, searching: bool) -> &'static str {
    match (state, searching) {
        ("posted", _) => "投稿済みの",
        ("drop", _) => "落とした",
        ("failed", _) => "失敗した",
        ("all", true) => "見つかった",
        ("all", false) => "全部で",
        _ => "投稿待ちの",
    }
}

fn item(article: &db::Article) -> String {
    let (kind, badge) = state(article);

    let mut meta = vec![
        escape(&crate::clock::jst_date(&article.collected_at)),
        escape(&article.source),
    ];
    if let Some(score) = article.score {
        meta.push(format!("{score}点"));
    }
    if let Some(count) = article.slide_count {
        meta.push(format!("スライド{count}枚"));
    } else if let Some(chars) = article.body_chars {
        meta.push(format!("{chars}字"));
    }
    if article.truncated {
        meta.push("上限で打ち切り".to_string());
    }
    if let Some(posted_at) = article.posted_at.as_deref() {
        let when = crate::clock::jst_datetime(posted_at);
        meta.push(match article.post_uri.as_deref() {
            Some(uri) => format!(
                "<a href=\"{}\">投稿 {}</a>",
                escape(&post_url(uri)),
                escape(&when)
            ),
            None => format!("投稿 {}", escape(&when)),
        });
    }

    let reason = match (
        &article.reason,
        &article.fetch_error,
        &article.judge_error,
        &article.post_error,
    ) {
        (_, Some(error), _, _) => format!(
            "<div class=\"reason\">取得できません: {}</div>",
            escape(error)
        ),
        (_, _, Some(error), _) => format!(
            "<div class=\"reason\">判定できません: {}</div>",
            escape(error)
        ),
        (_, _, _, Some(error)) => format!(
            "<div class=\"reason\">投稿できません: {}</div>",
            escape(error)
        ),
        (Some(reason), _, _, _) => format!("<div class=\"reason\">{}</div>", escape(reason)),
        _ => String::new(),
    };

    format!(
        "<article class=\"item {kind}\">
<div class=\"meta\">{}</div>
<div class=\"title\"><a href=\"{}\">{}</a>{badge}</div>
{}{reason}{}
</article>",
        meta.join(" · "),
        escape(&article.url),
        escape(&article.title),
        tags(article),
        post(article),
    )
}

fn tags(article: &db::Article) -> String {
    let mut tags = Vec::new();
    if let Some(basis) = article.basis.as_deref().filter(|basis| !basis.is_empty()) {
        tags.push(tag("basis", "関門", basis_label(basis)));
    }
    if let Some(depth) = article.depth.as_deref().filter(|depth| !depth.is_empty()) {
        let kind = match depth {
            "中心" => "depth center",
            "部分" => "depth part",
            _ => "depth hint",
        };
        tags.push(tag(kind, "展開", depth));
    }
    if let Some(field) = article.field.as_deref().filter(|field| !field.is_empty()) {
        tags.push(tag("field", "分野", field));
    }
    if let Some(form) = article.form.as_deref().filter(|form| !form.is_empty()) {
        tags.push(tag("form", "性質", form));
    }
    if tags.is_empty() {
        return String::new();
    }
    format!("<div class=\"tags\">{}</div>", tags.join(""))
}

fn tag(kind: &str, label: &str, value: &str) -> String {
    format!(
        "<span class=\"tag {kind}\"><i>{}</i>{}</span>",
        escape(label),
        escape(value)
    )
}

fn state(article: &db::Article) -> (&'static str, String) {
    if article.posted_at.is_some() {
        return (
            "pass",
            "<span class=\"badge pass\">投稿済み</span>".to_string(),
        );
    }
    if article.status == "expired" {
        return (
            "drop",
            "<span class=\"badge drop\">期限切れ</span>".to_string(),
        );
    }
    if article.fetch_error.is_some()
        || article.judge_error.is_some()
        || article.post_error.is_some()
    {
        return (
            "failed",
            "<span class=\"badge failed\">失敗</span>".to_string(),
        );
    }
    match article.verdict.as_deref() {
        Some("pass") => ("pass", "<span class=\"badge pass\">通す</span>".to_string()),
        Some("drop") => (
            "drop",
            "<span class=\"badge drop\">落とす</span>".to_string(),
        ),
        _ => ("", "<span class=\"badge drop\">未判定</span>".to_string()),
    }
}

fn post(article: &db::Article) -> String {
    if article.verdict.as_deref() != Some("pass") {
        return String::new();
    }
    let title = article.post_title.as_deref().unwrap_or("");
    let body = article.post_body.as_deref().unwrap_or("");
    let point = article.post_point.as_deref().unwrap_or("");
    if title.is_empty() && body.is_empty() {
        return String::new();
    }
    let draft = crate::post::draft(
        title,
        body,
        point,
        &article.url,
        article.card_title.as_deref().unwrap_or(&article.title),
        article.card_description.as_deref().unwrap_or(""),
        article.card_image.as_deref(),
    );
    let total = draft.text.chars().count();
    let warning = if crate::post::too_long(&draft) {
        format!(
            "<div class=\"warn\">{total}字で、Blueskyの上限{}字を超えています</div>",
            crate::post::TEXT_LIMIT
        )
    } else {
        format!("<div class=\"count\">{total}字</div>")
    };
    let text = escape(&draft.text);
    format!(
        "<div class=\"post\"><div class=\"label\">投稿文案</div><div class=\"body\">{}</div><a class=\"card\" href=\"{}\"><div class=\"card-title\">{}</div><div class=\"card-desc\">{}</div><div class=\"card-domain\">{}</div></a>{warning}<a class=\"record\" href=\"/record?id={}\">送るデータを見る</a></div>",
        text,
        escape(&article.url),
        escape(article.card_title.as_deref().unwrap_or(&article.title)),
        escape(article.card_description.as_deref().unwrap_or("")),
        escape(domain(&article.url)),
        article.id,
    )
}

fn post_url(uri: &str) -> String {
    match uri.rsplit('/').next() {
        Some(key) => format!("https://bsky.app/profile/tech-invariants.bsky.social/post/{key}"),
        None => uri.to_string(),
    }
}

fn basis_label(basis: &str) -> &'static str {
    match basis.trim().to_lowercase().as_str() {
        "a" => "a 普遍性",
        "b" => "b 仕組み・原理の理解",
        "c" => "c 通説を覆す",
        "d" => "d 発想の斬新さ",
        _ => "",
    }
}

fn domain(url: &str) -> &str {
    url.split('/').nth(2).unwrap_or(url)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn urlencode(text: &str) -> String {
    let mut encoded = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
