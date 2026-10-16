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
}

async fn index(State(pool): State<SqlitePool>, Query(view): Query<View>) -> Html<String> {
    let state = view.state.unwrap_or_else(|| "pass".to_string());
    match render(&pool, &state).await {
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
    let record = crate::post::record(&draft, &chrono::Utc::now().to_rfc3339());
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
header { position: sticky; top: 0; background: #fff; border-bottom: 1px solid #e3e6ea; padding: 1rem 1.5rem; }
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

async fn render(pool: &SqlitePool, state: &str) -> Result<String> {
    let counts = db::count_by_source(pool).await?;
    let (total, passed, dropped, failed, posted) = db::count_states(pool).await?;
    let articles = db::recent(pool, state, 300).await?;

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
        format!("<a class=\"tab{current}\" href=\"/?state={key}\">{label} <b>{count}</b></a>")
    })
    .collect::<String>();

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
<header><div class=\"counts\">{summary}</div><nav class=\"tabs\">{tabs}</nav></header>
<main>{items}</main>
</body>
</html>"
    ))
}

fn item(article: &db::Article) -> String {
    let (kind, badge) = state(article);

    let mut meta = vec![escape(&article.collected_at[..10]), escape(&article.source)];
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
    if let Some(field) = article.field.as_deref().filter(|f| !f.is_empty()) {
        meta.push(escape(field));
    }
    if let Some(form) = article.form.as_deref().filter(|f| !f.is_empty()) {
        meta.push(escape(form));
    }
    if let Some(basis) = article.basis.as_deref().filter(|b| !b.is_empty()) {
        meta.push(basis_label(basis).to_string());
    }
    if let Some(depth) = article.depth.as_deref().filter(|d| !d.is_empty()) {
        meta.push(format!("展開: {}", escape(depth)));
    }
    if let Some(posted_at) = article.posted_at.as_deref() {
        let when = posted_at.get(..16).unwrap_or(posted_at).replace('T', " ");
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
{reason}{}
</article>",
        meta.join(" · "),
        escape(&article.url),
        escape(&article.title),
        post(article),
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
