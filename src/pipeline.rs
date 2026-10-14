use crate::config::Config;
use crate::{bluesky, db, extract, http, judge, post, sources};
use anyhow::Result;
use sqlx::SqlitePool;
use std::time::Duration;

pub async fn run(config: &Config, pool: &SqlitePool) {
    log!("--- 収集を始めます");
    match sources::collect(config, pool).await {
        Ok(added) => log!("集めました {added}件"),
        Err(error) => log_error!("情報源から集められません: {error:#}"),
    }

    match db::expire_stale(pool, config.posting.expire_hours).await {
        Ok(n) if n > 0 => log!(
            "収集から{}時間を過ぎた{n}件を、投稿待ちから外しました",
            config.posting.expire_hours
        ),
        Err(error) => log_error!("期限切れの処理に失敗しました: {error:#}"),
        _ => {}
    }

    log!("--- 本文を取ります");
    match db::request_card_refetch(pool, config.collect.max_attempts).await {
        Ok(n) if n > 0 => log!("カード情報が無い{n}件を取り直します"),
        Err(error) => log_error!("カード情報の取り直しを頼めません: {error:#}"),
        _ => {}
    }
    loop {
        match extract_bodies(config, pool).await {
            Ok(0) => break,
            Ok(done) => log!("本文を取りました {done}件"),
            Err(error) => {
                log_error!("本文を取れません: {error:#}");
                break;
            }
        }
    }

    log!("--- 判定します");
    loop {
        match judge_articles(config, pool).await {
            Ok(Judged::Batch(0)) => break,
            Ok(Judged::Batch(done)) => log!("判定しました {done}件"),
            Ok(Judged::Unreachable) => {
                log!("判定の相手につながらないので、次の回にやり直します");
                break;
            }
            Err(error) => {
                log_error!("判定できません: {error:#}");
                break;
            }
        }
    }
}

enum Judged {
    Batch(usize),
    Unreachable,
}

async fn judge_articles(config: &Config, pool: &SqlitePool) -> Result<Judged> {
    let pending = db::pending_judge(pool, config.judge.batch, config.collect.max_attempts).await?;
    if pending.is_empty() {
        return Ok(Judged::Batch(0));
    }

    let client = http::client()?;
    let key = judge::api_key()?;

    let mut done = 0;
    for article in &pending {
        match judge::judge(
            &client,
            config,
            &key,
            &article.title,
            &article.source,
            &article.body,
        )
        .await
        {
            Ok(verdict) => db::save_verdict(pool, article.id, Some(&verdict), None).await?,
            Err(error) if extract::is_transient(&error) => {
                db::defer_verdict(pool, article.id, format!("{error:#}")).await?;
                return Ok(Judged::Unreachable);
            }
            Err(error) => {
                db::save_verdict(pool, article.id, None, Some(format!("{error:#}"))).await?
            }
        }
        done += 1;
    }
    Ok(Judged::Batch(done))
}

async fn extract_bodies(config: &Config, pool: &SqlitePool) -> Result<usize> {
    let client = http::client()?;
    let pending = db::pending_fetch(
        pool,
        config.collect.fetch_batch,
        config.collect.max_attempts,
    )
    .await?;
    let delay = Duration::from_secs(config.collect.fetch_delay_seconds);

    let mut done = 0;
    for article in &pending {
        match extract::extract(&client, &article.url, &config.limits).await {
            Ok(extract::Content::Text {
                text,
                truncated,
                card,
            }) => {
                db::save_body(
                    pool,
                    article.id,
                    db::Extracted {
                        body: Some(text),
                        truncated,
                        card_title: card.title,
                        card_description: Some(card.description),
                        card_image: card.image,
                        ..Default::default()
                    },
                )
                .await?;
            }
            Ok(extract::Content::Slides {
                urls,
                truncated,
                card,
            }) => {
                db::save_body(
                    pool,
                    article.id,
                    db::Extracted {
                        slide_urls: Some(serde_json::to_string(&urls)?),
                        truncated,
                        card_title: card.title,
                        card_description: Some(card.description),
                        card_image: card.image,
                        ..Default::default()
                    },
                )
                .await?;
            }
            Err(error) if extract::is_transient(&error) => {
                db::defer_body(pool, article.id, format!("{error:#}")).await?;
            }
            Err(error) => {
                db::save_body(
                    pool,
                    article.id,
                    db::Extracted {
                        error: Some(format!("{error:#}")),
                        ..Default::default()
                    },
                )
                .await?;
            }
        }
        done += 1;
        tokio::time::sleep(delay).await;
    }
    Ok(done)
}

pub async fn wait_until_due(config: &Config, pool: &SqlitePool) {
    let interval = chrono::Duration::minutes(config.posting.interval_minutes as i64);
    let Ok(Some(last)) = db::last_posted_at(pool).await else {
        return;
    };
    let Ok(last) = chrono::DateTime::parse_from_rfc3339(&last) else {
        return;
    };

    let due = last.with_timezone(&chrono::Utc) + interval;
    let mut announced = false;
    loop {
        let remaining = due - chrono::Utc::now();
        if remaining <= chrono::Duration::zero() {
            return;
        }
        if !announced {
            let minutes = remaining.num_minutes() + 1;
            log!("前回の投稿から間がないため、{minutes}分後に投稿します");
            announced = true;
        }
        let step = remaining.min(chrono::Duration::minutes(1));
        match step.to_std() {
            Ok(duration) => tokio::time::sleep(duration).await,
            Err(_) => return,
        }
    }
}

pub async fn sleep_watching(duration: Duration) {
    let before = chrono::Utc::now();
    tokio::time::sleep(duration).await;
    let Ok(expected) = chrono::Duration::from_std(duration) else {
        return;
    };
    if let Some(gap) = gap_to_notice(chrono::Utc::now() - before, expected) {
        log!(
            "{}ぶん止まっていました（スリープか時刻の飛び）",
            crate::log::gap_text(gap)
        );
    }
}

pub fn gap_to_notice(
    slept: chrono::Duration,
    expected: chrono::Duration,
) -> Option<chrono::Duration> {
    let gap = slept - expected;
    (gap > chrono::Duration::minutes(2)).then_some(gap)
}

pub const RETRY_SECONDS: u64 = 120;

pub enum Posting {
    Done,
    Nothing,
    Failed,
}

pub fn next_wait(result: &Posting, interval: Duration) -> Duration {
    match result {
        Posting::Failed => Duration::from_secs(RETRY_SECONDS),
        Posting::Done | Posting::Nothing => interval,
    }
}

pub async fn post_batch(config: &Config, pool: &SqlitePool) -> Posting {
    let gap = Duration::from_secs(config.posting.gap_seconds);

    if let Ok(waiting) = db::queued_count(pool).await {
        log!("投稿待ち {waiting}件");
    }

    match db::request_card_refetch(pool, config.collect.max_attempts).await {
        Ok(n) if n > 0 => log!("カード情報が無い{n}件は、取り直すまで投稿を見送ります"),
        Err(error) => log_error!("カード情報の取り直しを頼めません: {error:#}"),
        _ => {}
    }

    for _ in 0..config.posting.per_batch {
        match db::posted_today(pool).await {
            Ok(count) if count >= config.posting.daily_limit => {
                log!(
                    "1日の上限（{}件）に達しています",
                    config.posting.daily_limit
                );
                return Posting::Nothing;
            }
            Err(error) => {
                log_error!("投稿できません: {error:#}");
                return Posting::Failed;
            }
            _ => {}
        }

        match post_one(config, pool).await {
            Ok(false) => return Posting::Nothing,
            Ok(true) => {}
            Err(error) => {
                log_error!("投稿できません: {error:#}");
                return Posting::Failed;
            }
        }
        tokio::time::sleep(gap).await;
    }
    Posting::Done
}

pub async fn sync_posted(config: &Config, pool: &SqlitePool) -> Result<usize> {
    let client = http::client()?;
    let links = bluesky::posted_links(&client, config).await?;

    let mut repaired = 0;
    for link in &links {
        if db::mark_posted_by_url(pool, link).await? {
            repaired += 1;
        }
    }
    if repaired > 0 {
        log!("Bluesky に投稿済みだった{repaired}件を、投稿待ちから外しました");
    }
    Ok(repaired)
}

pub async fn post_one(config: &Config, pool: &SqlitePool) -> Result<bool> {
    let Some((id, queued)) = db::next_queued(pool, config.collect.max_attempts).await? else {
        log!("投稿待ちの記事がありません");
        return Ok(false);
    };

    let draft = post::draft(
        &queued.post_title,
        &queued.post_body,
        &queued.post_point,
        &queued.url,
        queued.card_title.as_deref().unwrap_or(&queued.title),
        &queued.card_description,
        queued.card_image.as_deref(),
    );
    if post::empty(&draft) {
        let error = format!("本文が空です（記事 {id}）");
        db::note_post_error(pool, id, error.clone(), true).await?;
        return Err(anyhow::anyhow!(error));
    }
    if post::too_long(&draft) {
        let error = format!("本文が{}字で上限を超えています", draft.text.chars().count());
        db::note_post_error(pool, id, error.clone(), true).await?;
        return Err(anyhow::anyhow!(error));
    }

    let record = post::record(&draft, &chrono::Utc::now().to_rfc3339());
    let client = http::client()?;
    let password = bluesky::app_password()?;
    let posted = bluesky::post(
        &client,
        config,
        &password,
        &record,
        draft.card_image.as_deref(),
    )
    .await;

    let uri = match posted {
        Ok(uri) => uri,
        Err(error) if bluesky::is_thumb_unusable(&error) => {
            db::clear_card_image(pool, id).await?;
            log!("画像が使えないので、画像なしで出します（記事 {id}）: {error:#}");
            bluesky::post(&client, config, &password, &record, None).await?
        }
        Err(error) => {
            let counts = !extract::is_transient(&error);
            db::note_post_error(pool, id, format!("{error:#}"), counts).await?;
            return Err(error);
        }
    };

    db::mark_posted(pool, id, &uri).await?;
    log!("{}", bluesky::web_url(&config.bluesky.handle, &uri));
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 通信で流れた回のあとは短い間隔で試し直し、出せた回・出すものが無い回はいつもの間隔で待つこと
    #[test]
    fn waits_shorter_after_failure() {
        let interval = Duration::from_secs(30 * 60);

        assert_eq!(
            next_wait(&Posting::Failed, interval),
            Duration::from_secs(RETRY_SECONDS)
        );
        assert_eq!(next_wait(&Posting::Done, interval), interval);
        assert_eq!(next_wait(&Posting::Nothing, interval), interval);
        assert!(Duration::from_secs(RETRY_SECONDS) < interval);
    }

    /// 待つはずだった時間より2分を超えて過ぎていたら、止まっていた時間として知らせること
    #[test]
    fn notices_long_gaps() {
        let expected = chrono::Duration::minutes(30);

        assert_eq!(
            gap_to_notice(chrono::Duration::minutes(180), expected),
            Some(chrono::Duration::minutes(150))
        );
        assert_eq!(gap_to_notice(chrono::Duration::minutes(31), expected), None);
        assert_eq!(gap_to_notice(expected, expected), None);
    }
}
