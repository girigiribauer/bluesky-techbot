use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

pub struct NewArticle {
    pub source: String,
    pub url: String,
    pub title: String,
    pub score: Option<i64>,
    pub published_at: Option<String>,
}

pub struct Article {
    pub id: i64,
    pub source: String,
    pub url: String,
    pub title: String,
    pub score: Option<i64>,
    pub collected_at: String,
    pub status: String,
    pub body_chars: Option<i64>,
    pub slide_count: Option<i64>,
    pub truncated: bool,
    pub fetch_error: Option<String>,
    pub verdict: Option<String>,
    pub reason: Option<String>,
    pub field: Option<String>,
    pub form: Option<String>,
    pub basis: Option<String>,
    pub depth: Option<String>,
    pub posted_at: Option<String>,
    pub post_uri: Option<String>,
    pub judge_error: Option<String>,
    pub post_error: Option<String>,
    pub post_title: Option<String>,
    pub post_body: Option<String>,
    pub post_point: Option<String>,
    pub card_title: Option<String>,
    pub card_description: Option<String>,
    pub card_image: Option<String>,
}

pub struct Queued {
    pub url: String,
    pub title: String,
    pub card_title: Option<String>,
    pub card_description: String,
    pub card_image: Option<String>,
    pub post_title: String,
    pub post_body: String,
    pub post_point: String,
}

pub struct Pending {
    pub id: i64,
    pub url: String,
}

pub struct Unjudged {
    pub id: i64,
    pub source: String,
    pub title: String,
    pub body: String,
    pub card_title: Option<String>,
    pub post_title: Option<String>,
}

pub async fn connect(path: &str) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(10));
    let pool = SqlitePoolOptions::new().connect_with(options).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS articles (
            id INTEGER PRIMARY KEY,
            source TEXT NOT NULL,
            url TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL,
            score INTEGER,
            published_at TEXT,
            collected_at TEXT NOT NULL,
            status TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await?;
    migrate(&pool).await?;
    Ok(pool)
}

async fn migrate(pool: &SqlitePool) -> Result<()> {
    let existing: Vec<String> = sqlx::query("PRAGMA table_info(articles)")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.get("name"))
        .collect();

    let columns = [
        ("body", "TEXT"),
        ("slide_urls", "TEXT"),
        ("truncated", "INTEGER NOT NULL DEFAULT 0"),
        ("fetch_error", "TEXT"),
        ("fetched_at", "TEXT"),
        ("verdict", "TEXT"),
        ("reason", "TEXT"),
        ("field", "TEXT"),
        ("industry", "TEXT"),
        ("form", "TEXT"),
        ("judge_error", "TEXT"),
        ("post_title", "TEXT"),
        ("post_body", "TEXT"),
        ("post_point", "TEXT"),
        ("basis", "TEXT"),
        ("fetch_attempts", "INTEGER NOT NULL DEFAULT 0"),
        ("judge_attempts", "INTEGER NOT NULL DEFAULT 0"),
        ("depth", "TEXT"),
        ("post_uri", "TEXT"),
        ("posted_at", "TEXT"),
        ("card_title", "TEXT"),
        ("card_description", "TEXT"),
        ("card_image", "TEXT"),
        ("judged_at", "TEXT"),
        ("post_error", "TEXT"),
        ("post_attempts", "INTEGER NOT NULL DEFAULT 0"),
    ];
    for (name, kind) in columns {
        if !existing.iter().any(|column| column == name) {
            sqlx::query(&format!("ALTER TABLE articles ADD COLUMN {name} {kind}"))
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

pub async fn insert(pool: &SqlitePool, article: &NewArticle) -> Result<bool> {
    let duplicate: Option<i64> = sqlx::query("SELECT id FROM articles WHERE title = ? LIMIT 1")
        .bind(&article.title)
        .fetch_optional(pool)
        .await?
        .map(|row| row.get("id"));
    if duplicate.is_some() {
        return Ok(false);
    }

    let result = sqlx::query(
        "INSERT OR IGNORE INTO articles
            (source, url, title, score, published_at, collected_at, status)
         VALUES (?, ?, ?, ?, ?, ?, 'collected')",
    )
    .bind(&article.source)
    .bind(&article.url)
    .bind(&article.title)
    .bind(article.score)
    .bind(&article.published_at)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

const SEARCH_MATCH: &str = "(title LIKE '%' || ? || '%'
     OR url LIKE '%' || ? || '%'
     OR COALESCE(post_title, '') LIKE '%' || ? || '%'
     OR COALESCE(post_body, '') LIKE '%' || ? || '%'
     OR COALESCE(post_point, '') LIKE '%' || ? || '%')";

pub fn state_condition(state: &str) -> &'static str {
    match state {
        "pass" => "verdict = 'pass' AND status = 'queued'",
        "posted" => "status = 'posted'",
        "drop" => "verdict = 'drop' OR status = 'expired'",
        "failed" => "fetch_error IS NOT NULL OR judge_error IS NOT NULL OR post_error IS NOT NULL",
        _ => "1 = 1",
    }
}

pub async fn recent(
    pool: &SqlitePool,
    state: &str,
    search: Option<&str>,
    limit: i64,
) -> Result<Vec<Article>> {
    let filter = match search {
        Some(_) => format!(" AND {SEARCH_MATCH}"),
        None => String::new(),
    };
    let sql = format!(
        "SELECT id, source, url, title, score, collected_at, status,
                LENGTH(body) AS body_chars,
                json_array_length(slide_urls) AS slide_count,
                truncated, fetch_error, posted_at, post_uri,
                verdict, reason, field, form, basis, depth, judge_error, post_error,
                post_title, post_body, post_point,
                card_title, card_description, card_image
         FROM articles
         WHERE ({}){filter}
         ORDER BY CASE WHEN ? = 'posted' THEN posted_at END DESC, collected_at DESC, id DESC
         LIMIT ?",
        state_condition(state)
    );
    let mut query = sqlx::query(&sql);
    if let Some(word) = search {
        for _ in 0..5 {
            query = query.bind(word);
        }
    }
    let rows = query.bind(state).bind(limit).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| Article {
            id: row.get("id"),
            source: row.get("source"),
            url: row.get("url"),
            title: row.get("title"),
            score: row.get("score"),
            collected_at: row.get("collected_at"),
            status: row.get("status"),
            body_chars: row.get("body_chars"),
            slide_count: row.get("slide_count"),
            truncated: row.get::<i64, _>("truncated") != 0,
            fetch_error: row.get("fetch_error"),
            verdict: row.get("verdict"),
            reason: row.get("reason"),
            field: row.get("field"),
            form: row.get("form"),
            basis: row.get("basis"),
            depth: row.get("depth"),
            posted_at: row.get("posted_at"),
            post_uri: row.get("post_uri"),
            judge_error: row.get("judge_error"),
            post_error: row.get("post_error"),
            post_title: row.get("post_title"),
            post_body: row.get("post_body"),
            post_point: row.get("post_point"),
            card_title: row.get("card_title"),
            card_description: row.get("card_description"),
            card_image: row.get("card_image"),
        })
        .collect())
}

pub async fn count_states(
    pool: &SqlitePool,
    search: Option<&str>,
) -> Result<(i64, i64, i64, i64, i64)> {
    let sql = format!(
        "SELECT COUNT(*) AS total,
                SUM(verdict = 'pass' AND status = 'queued') AS passed,
                SUM(verdict = 'drop' OR status = 'expired') AS dropped,
                SUM(fetch_error IS NOT NULL OR judge_error IS NOT NULL OR post_error IS NOT NULL) AS failed,
                SUM(status = 'posted') AS posted
         FROM articles
         WHERE {}",
        if search.is_some() { SEARCH_MATCH } else { "1 = 1" }
    );
    let mut query = sqlx::query(&sql);
    if let Some(word) = search {
        for _ in 0..5 {
            query = query.bind(word);
        }
    }
    let row = query.fetch_one(pool).await?;
    Ok((
        row.get("total"),
        row.try_get("passed").unwrap_or(0),
        row.try_get("dropped").unwrap_or(0),
        row.try_get("failed").unwrap_or(0),
        row.try_get("posted").unwrap_or(0),
    ))
}

pub async fn note_post_error(
    pool: &SqlitePool,
    id: i64,
    error: String,
    counts: bool,
) -> Result<()> {
    sqlx::query(
        "UPDATE articles
         SET post_error = ?, post_attempts = post_attempts + ?
         WHERE id = ?",
    )
    .bind(error)
    .bind(i64::from(counts))
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn clear_card_image(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query("UPDATE articles SET card_image = NULL WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn expire_stale(pool: &SqlitePool, hours: i64) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE articles
         SET status = 'expired'
         WHERE status = 'queued' AND posted_at IS NULL AND collected_at < ?",
    )
    .bind((chrono::Utc::now() - chrono::Duration::hours(hours)).to_rfc3339())
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn request_card_refetch(pool: &SqlitePool, max_attempts: i64) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE articles
         SET fetched_at = NULL
         WHERE status = 'queued' AND posted_at IS NULL
           AND (card_title IS NULL OR card_title = '')
           AND fetch_attempts < ?",
    )
    .bind(max_attempts)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn next_queued(pool: &SqlitePool, max_attempts: i64) -> Result<Option<(i64, Queued)>> {
    let row = sqlx::query(
        "SELECT id, url, title, card_title, card_description, card_image, post_title, post_body, post_point
         FROM articles
         WHERE status = 'queued' AND verdict = 'pass' AND posted_at IS NULL
           AND fetched_at IS NOT NULL AND post_attempts < ?
         ORDER BY id ASC
         LIMIT 1",
    )
    .bind(max_attempts)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| {
        (
            row.get("id"),
            Queued {
                url: row.get("url"),
                title: row.get("title"),
                card_title: row.try_get("card_title").unwrap_or_default(),
                card_description: row.try_get("card_description").unwrap_or_default(),
                card_image: row.try_get("card_image").unwrap_or_default(),
                post_title: row.try_get("post_title").unwrap_or_default(),
                post_body: row.try_get("post_body").unwrap_or_default(),
                post_point: row.try_get("post_point").unwrap_or_default(),
            },
        )
    }))
}

pub async fn mark_posted_by_url(pool: &SqlitePool, url: &str) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE articles
         SET status = 'posted', posted_at = COALESCE(posted_at, ?)
         WHERE url = ? AND status <> 'posted'",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(url)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn queued_count(pool: &SqlitePool) -> Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) AS n FROM articles WHERE status = 'queued'")
        .fetch_one(pool)
        .await?;
    Ok(row.get("n"))
}

pub async fn last_posted_at(pool: &SqlitePool) -> Result<Option<String>> {
    let row =
        sqlx::query("SELECT MAX(posted_at) AS last FROM articles WHERE posted_at IS NOT NULL")
            .fetch_one(pool)
            .await?;
    Ok(row.try_get("last").ok().flatten())
}

pub async fn posted_today(pool: &SqlitePool) -> Result<i64> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS n FROM articles
         WHERE posted_at IS NOT NULL AND posted_at > ?",
    )
    .bind((chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339())
    .fetch_one(pool)
    .await?;
    Ok(row.get("n"))
}

pub async fn mark_posted(pool: &SqlitePool, id: i64, uri: &str) -> Result<()> {
    sqlx::query(
        "UPDATE articles
         SET status = 'posted', post_uri = ?, posted_at = ?, post_error = NULL
         WHERE id = ?",
    )
    .bind(uri)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn queued(pool: &SqlitePool, id: i64) -> Result<Option<Queued>> {
    let row = sqlx::query(
        "SELECT url, title, card_title, card_description, card_image, post_title, post_body, post_point
         FROM articles WHERE id = ? AND verdict = 'pass'",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| Queued {
        url: row.get("url"),
        title: row.get("title"),
        card_title: row.try_get("card_title").unwrap_or_default(),
        card_description: row.try_get("card_description").unwrap_or_default(),
        card_image: row.try_get("card_image").unwrap_or_default(),
        post_title: row.try_get("post_title").unwrap_or_default(),
        post_body: row.try_get("post_body").unwrap_or_default(),
        post_point: row.try_get("post_point").unwrap_or_default(),
    }))
}

pub async fn pending_fetch(
    pool: &SqlitePool,
    limit: i64,
    max_attempts: i64,
) -> Result<Vec<Pending>> {
    let rows = sqlx::query(
        "SELECT id, url FROM articles
         WHERE fetched_at IS NULL AND fetch_attempts < ?
         ORDER BY id DESC
         LIMIT ?",
    )
    .bind(max_attempts)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| Pending {
            id: row.get("id"),
            url: row.get("url"),
        })
        .collect())
}

pub async fn pending_judge(
    pool: &SqlitePool,
    limit: i64,
    max_attempts: i64,
) -> Result<Vec<Unjudged>> {
    let rows = sqlx::query(
        "SELECT id, source, title, body, card_title, post_title FROM articles
         WHERE body IS NOT NULL AND verdict IS NULL AND judge_attempts < ?
         ORDER BY id DESC
         LIMIT ?",
    )
    .bind(max_attempts)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| Unjudged {
            id: row.get("id"),
            source: row.get("source"),
            title: row.get("title"),
            body: row.get("body"),
            card_title: row.get("card_title"),
            post_title: row.get("post_title"),
        })
        .collect())
}

pub async fn save_verdict(
    pool: &SqlitePool,
    id: i64,
    verdict: Option<&crate::judge::Verdict>,
    error: Option<String>,
) -> Result<()> {
    sqlx::query(
        "UPDATE articles
         SET verdict = ?, reason = ?, field = ?, form = ?,
             basis = ?, depth = ?, post_title = ?, post_body = ?, post_point = ?,
             judge_error = ?, judged_at = ?, judge_attempts = judge_attempts + 1,
             status = CASE
                 WHEN posted_at IS NOT NULL THEN status
                 WHEN status = 'expired' THEN status
                 WHEN ? = 'pass' THEN 'queued'
                 WHEN ? = 'drop' THEN 'dropped'
                 ELSE status
             END
         WHERE id = ?",
    )
    .bind(verdict.map(|v| if v.pass { "pass" } else { "drop" }))
    .bind(verdict.map(|v| v.reason.clone()))
    .bind(verdict.map(|v| v.field.clone()))
    .bind(verdict.map(|v| v.form.clone()))
    .bind(verdict.map(|v| v.basis.clone()))
    .bind(verdict.map(|v| v.depth.clone()))
    .bind(verdict.map(|v| v.post_title.clone()))
    .bind(verdict.map(|v| v.post_body.clone()))
    .bind(verdict.map(|v| v.post_point.clone()))
    .bind(error)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(verdict.map(|v| if v.pass { "pass" } else { "drop" }))
    .bind(verdict.map(|v| if v.pass { "pass" } else { "drop" }))
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Default)]
pub struct Extracted {
    pub body: Option<String>,
    pub card_title: Option<String>,
    pub slide_urls: Option<String>,
    pub truncated: bool,
    pub card_description: Option<String>,
    pub card_image: Option<String>,
    pub error: Option<String>,
}

pub async fn save_body(pool: &SqlitePool, id: i64, extracted: Extracted) -> Result<()> {
    sqlx::query(
        "UPDATE articles
         SET body = ?, slide_urls = ?, truncated = ?,
             card_title = ?, card_description = ?, card_image = ?,
             fetch_error = ?, fetched_at = ?,
             fetch_attempts = fetch_attempts + 1
         WHERE id = ?",
    )
    .bind(extracted.body)
    .bind(extracted.slide_urls)
    .bind(extracted.truncated as i64)
    .bind(extracted.card_title)
    .bind(extracted.card_description)
    .bind(extracted.card_image)
    .bind(extracted.error)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn defer_body(pool: &SqlitePool, id: i64, error: String) -> Result<()> {
    sqlx::query(
        "UPDATE articles
         SET fetch_error = ?, fetch_attempts = fetch_attempts + 1
         WHERE id = ?",
    )
    .bind(error)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn defer_verdict(pool: &SqlitePool, id: i64, error: String) -> Result<()> {
    sqlx::query("UPDATE articles SET judge_error = ? WHERE id = ?")
        .bind(error)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn count_by_source(pool: &SqlitePool) -> Result<Vec<(String, i64)>> {
    let rows =
        sqlx::query("SELECT source, COUNT(*) AS n FROM articles GROUP BY source ORDER BY n DESC")
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get("source"), row.get("n")))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::judge::Verdict;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    async fn test_pool() -> SqlitePool {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("techbot-test-{}-{n}.db", std::process::id()));
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        connect(path.to_str().unwrap()).await.unwrap()
    }

    async fn add(pool: &SqlitePool, id: i64, status: &str, verdict: &str, posted_at: Option<&str>) {
        sqlx::query(
            "INSERT INTO articles
                (id, source, url, title, collected_at, status, verdict, posted_at, fetched_at,
                 card_title, post_title, post_body, post_point, card_description)
             VALUES (?, 'test', ?, ?, ?, ?, ?, ?,
                     ?, 'カード題名', '題名', '要約', '要点', '紹介文')",
        )
        .bind(id)
        .bind(format!("https://example.com/{id}"))
        .bind(format!("記事{id}"))
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(status)
        .bind(verdict)
        .bind(posted_at)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .unwrap();
    }

    async fn add_fetched(pool: &SqlitePool, id: i64, judged_at: Option<&str>, judge_attempts: i64) {
        sqlx::query(
            "INSERT INTO articles
                (id, source, url, title, collected_at, status, body, judged_at, judge_attempts)
             VALUES (?, 'test', ?, ?, ?, 'collected', '本文', ?, ?)",
        )
        .bind(id)
        .bind(format!("https://example.com/{id}"))
        .bind(format!("記事{id}"))
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(judged_at)
        .bind(judge_attempts)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn clear_card(pool: &SqlitePool, id: i64, fetch_attempts: i64) {
        sqlx::query(
            "UPDATE articles SET card_title = '', card_image = '', fetch_attempts = ? WHERE id = ?",
        )
        .bind(fetch_attempts)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn collected_days_ago(pool: &SqlitePool, id: i64, days: i64) {
        sqlx::query("UPDATE articles SET collected_at = ? WHERE id = ?")
            .bind((chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339())
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn fetched_at_of(pool: &SqlitePool, id: i64) -> Option<String> {
        sqlx::query("SELECT fetched_at FROM articles WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
            .get("fetched_at")
    }

    async fn status_of(pool: &SqlitePool, id: i64) -> String {
        sqlx::query("SELECT status FROM articles WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
            .get("status")
    }

    fn verdict(pass: bool) -> Verdict {
        Verdict {
            pass,
            reason: "理由".into(),
            field: "分野".into(),
            form: "形".into(),
            basis: "根拠".into(),
            depth: "深さ".into(),
            post_title: "題名".into(),
            post_body: "要約".into(),
            post_point: "要点".into(),
        }
    }

    /// すでに投稿した記事は、投稿待ちの印が付いていても次の投稿には選ばれないこと
    #[tokio::test]
    async fn never_posts_twice() {
        let pool = test_pool().await;
        add(&pool, 1, "queued", "pass", Some("2026-10-13T00:00:00Z")).await;
        add(&pool, 2, "queued", "pass", None).await;

        let (id, _) = next_queued(&pool, 3).await.unwrap().unwrap();

        assert_eq!(id, 2);
    }

    /// まだ投稿していない「通す」記事は、古いものから選ばれること
    #[tokio::test]
    async fn picks_oldest_unposted() {
        let pool = test_pool().await;
        add(&pool, 3, "queued", "pass", None).await;
        add(&pool, 4, "queued", "pass", None).await;

        let (id, _) = next_queued(&pool, 3).await.unwrap().unwrap();

        assert_eq!(id, 3);
    }

    /// 本文があって判定が付いていない記事は、判定待ちに出ること
    #[tokio::test]
    async fn waits_for_first_judge() {
        let pool = test_pool().await;
        add_fetched(&pool, 1, None, 0).await;

        let pending = pending_judge(&pool, 10, 3).await.unwrap();

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, 1);
    }

    /// 判定が失敗して判定の印だけ付いた記事も、やり直しの上限までは判定待ちに出ること
    #[tokio::test]
    async fn retries_failed_judge() {
        let pool = test_pool().await;
        add_fetched(&pool, 1, Some("2026-10-16T00:00:00Z"), 1).await;

        let pending = pending_judge(&pool, 10, 3).await.unwrap();

        assert_eq!(pending.len(), 1);
    }

    /// やり直しの回数が上限に達した記事は、判定待ちに出ないこと
    #[tokio::test]
    async fn gives_up_judging_after_max_attempts() {
        let pool = test_pool().await;
        add_fetched(&pool, 1, Some("2026-10-16T00:00:00Z"), 3).await;

        let pending = pending_judge(&pool, 10, 3).await.unwrap();

        assert!(pending.is_empty());
    }

    /// 判定が付いた記事は、判定待ちに出ないこと
    #[tokio::test]
    async fn skips_judged_article() {
        let pool = test_pool().await;
        add_fetched(&pool, 1, Some("2026-10-16T00:00:00Z"), 1).await;
        sqlx::query("UPDATE articles SET verdict = 'drop' WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();

        let pending = pending_judge(&pool, 10, 3).await.unwrap();

        assert!(pending.is_empty());
    }

    /// 検索語を渡すと、題名か投稿文に含む記事だけを返すこと
    #[tokio::test]
    async fn searches_title_and_post_text() {
        let pool = test_pool().await;
        add(&pool, 1, "queued", "pass", None).await;
        add(&pool, 2, "queued", "pass", None).await;
        sqlx::query("UPDATE articles SET title = '量子計算の入門' WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE articles SET post_point = 'キャッシュの話です' WHERE id = 2")
            .execute(&pool)
            .await
            .unwrap();

        let by_title = recent(&pool, "pass", Some("量子"), 10).await.unwrap();
        let by_point = recent(&pool, "pass", Some("キャッシュ"), 10).await.unwrap();

        assert_eq!(by_title.len(), 1);
        assert_eq!(by_title[0].id, 1);
        assert_eq!(by_point.len(), 1);
        assert_eq!(by_point[0].id, 2);
    }

    /// 検索語が無ければ、そのタブの記事をすべて返すこと
    #[tokio::test]
    async fn lists_all_without_search() {
        let pool = test_pool().await;
        add(&pool, 1, "queued", "pass", None).await;
        add(&pool, 2, "queued", "pass", None).await;

        assert_eq!(recent(&pool, "pass", None, 10).await.unwrap().len(), 2);
    }

    /// 落とすタブのように条件が OR のときも、検索で絞り込めること
    #[tokio::test]
    async fn searches_within_dropped() {
        let pool = test_pool().await;
        add(&pool, 1, "dropped", "drop", None).await;
        add(&pool, 2, "queued", "pass", None).await;
        sqlx::query("UPDATE articles SET title = '量子計算の入門' WHERE id = 2")
            .execute(&pool)
            .await
            .unwrap();

        assert!(
            recent(&pool, "drop", Some("量子"), 10)
                .await
                .unwrap()
                .is_empty()
        );
    }

    /// 判定をやり直しても、期限切れの記事は期限切れのまま残ること
    #[tokio::test]
    async fn rejudge_keeps_expired() {
        let pool = test_pool().await;
        add(&pool, 1, "expired", "pass", None).await;

        save_verdict(&pool, 1, Some(&verdict(true)), None)
            .await
            .unwrap();

        assert_eq!(status_of(&pool, 1).await, "expired");
    }

    /// 判定をやり直しても、投稿済みの記事は投稿済みのまま残ること
    #[tokio::test]
    async fn rejudge_keeps_posted() {
        let pool = test_pool().await;
        add(&pool, 1, "posted", "pass", Some("2026-10-13T00:00:00Z")).await;

        save_verdict(&pool, 1, Some(&verdict(true)), None)
            .await
            .unwrap();

        assert_eq!(status_of(&pool, 1).await, "posted");
    }

    /// 通信が切れて投稿できなかったときは、投稿の試行回数を増やさないこと
    #[tokio::test]
    async fn transient_post_error_keeps_attempts() {
        let pool = test_pool().await;
        add(&pool, 1, "queued", "pass", None).await;

        note_post_error(&pool, 1, "接続できません".to_string(), false)
            .await
            .unwrap();

        let (id, _) = next_queued(&pool, 3).await.unwrap().unwrap();
        assert_eq!(id, 1);
    }

    /// 記事そのものが原因で投稿できない記事は、何度か試したら投稿待ちから外し、次の記事に進むこと
    #[tokio::test]
    async fn skips_article_that_cannot_be_posted() {
        let pool = test_pool().await;
        add(&pool, 1, "queued", "pass", None).await;
        add(&pool, 2, "queued", "pass", None).await;

        for _ in 0..3 {
            note_post_error(&pool, 1, "本文が空です".to_string(), true)
                .await
                .unwrap();
        }

        let (id, _) = next_queued(&pool, 3).await.unwrap().unwrap();
        assert_eq!(id, 2);
    }

    /// 投稿できたら、前に失敗した記録は消すこと
    #[tokio::test]
    async fn clears_post_error_when_posted() {
        let pool = test_pool().await;
        add(&pool, 1, "queued", "pass", None).await;
        note_post_error(&pool, 1, "接続できません".to_string(), false)
            .await
            .unwrap();

        mark_posted(&pool, 1, "at://example/post/1").await.unwrap();

        let error: Option<String> = sqlx::query("SELECT post_error FROM articles WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("post_error");
        assert_eq!(error, None);
    }

    /// 判定の相手につながらなかったときは、記事の試行回数を増やさないこと
    #[tokio::test]
    async fn transient_judge_error_keeps_attempts() {
        let pool = test_pool().await;
        add_fetched(&pool, 1, None, 0).await;

        defer_verdict(&pool, 1, "APIに接続できません".to_string())
            .await
            .unwrap();

        let pending = pending_judge(&pool, 10, 3).await.unwrap();
        assert_eq!(pending.len(), 1);
    }

    /// 収集から期限を過ぎた投稿待ちの記事は、期限切れになって投稿に選ばれないこと
    #[tokio::test]
    async fn expires_stale_queued() {
        let pool = test_pool().await;
        add(&pool, 1, "queued", "pass", None).await;
        add(&pool, 2, "queued", "pass", None).await;
        collected_days_ago(&pool, 1, 4).await;

        let expired = expire_stale(&pool, 72).await.unwrap();
        let (id, _) = next_queued(&pool, 3).await.unwrap().unwrap();

        assert_eq!(expired, 1);
        assert_eq!(status_of(&pool, 1).await, "expired");
        assert_eq!(id, 2);
    }

    /// すでに投稿した記事は、収集が古くても期限切れにしないこと
    #[tokio::test]
    async fn keeps_posted_when_stale() {
        let pool = test_pool().await;
        add(&pool, 1, "posted", "pass", Some("2026-10-13T00:00:00Z")).await;
        collected_days_ago(&pool, 1, 4).await;

        let expired = expire_stale(&pool, 72).await.unwrap();

        assert_eq!(expired, 0);
        assert_eq!(status_of(&pool, 1).await, "posted");
    }

    /// カード情報を一度も取っていない記事には、取り直しの印が付くこと
    #[tokio::test]
    async fn asks_refetch_when_card_missing() {
        let pool = test_pool().await;
        add(&pool, 1, "queued", "pass", None).await;
        add(&pool, 2, "queued", "pass", None).await;
        clear_card(&pool, 1, 0).await;

        let asked = request_card_refetch(&pool, 3).await.unwrap();

        assert_eq!(asked, 1);
        assert_eq!(fetched_at_of(&pool, 1).await, None);
        assert!(fetched_at_of(&pool, 2).await.is_some());
    }

    /// 取り直し待ちの記事は、取り直すまで投稿に選ばれないこと
    #[tokio::test]
    async fn waits_for_card_before_posting() {
        let pool = test_pool().await;
        add(&pool, 1, "queued", "pass", None).await;
        add(&pool, 2, "queued", "pass", None).await;
        clear_card(&pool, 1, 0).await;
        request_card_refetch(&pool, 3).await.unwrap();

        let (id, _) = next_queued(&pool, 3).await.unwrap().unwrap();

        assert_eq!(id, 2);
    }

    /// 何度取り直してもカード情報が取れない記事は、諦めてそのまま投稿に選ばれること
    #[tokio::test]
    async fn gives_up_after_attempts() {
        let pool = test_pool().await;
        add(&pool, 1, "queued", "pass", None).await;
        clear_card(&pool, 1, 3).await;

        let asked = request_card_refetch(&pool, 3).await.unwrap();
        let (id, _) = next_queued(&pool, 3).await.unwrap().unwrap();

        assert_eq!(asked, 0);
        assert_eq!(id, 1);
    }

    /// まだ投稿していない記事の判定は、通れば投稿待ち・落とせば落とすの印が付くこと
    #[tokio::test]
    async fn verdict_moves_unposted() {
        let pool = test_pool().await;
        add(&pool, 1, "collected", "", None).await;
        add(&pool, 2, "collected", "", None).await;

        save_verdict(&pool, 1, Some(&verdict(true)), None)
            .await
            .unwrap();
        save_verdict(&pool, 2, Some(&verdict(false)), None)
            .await
            .unwrap();

        assert_eq!(status_of(&pool, 1).await, "queued");
        assert_eq!(status_of(&pool, 2).await, "dropped");
    }
}
