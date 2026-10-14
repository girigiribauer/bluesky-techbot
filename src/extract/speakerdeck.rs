use super::{Card, Content, get_text, normalize};
use crate::config::Limits;
use anyhow::{Result, anyhow};
use regex::Regex;

pub async fn extract(client: &reqwest::Client, url: &str, limits: &Limits) -> Result<Content> {
    let page = get_text(client, url).await?;

    if let Some(text) = pdf_text(client, &page, limits).await? {
        return Ok(Content::Text {
            text,
            truncated: false,
            card: super::card(&page, url),
        });
    }
    slide_images(client, &page, limits.slides).await
}

async fn pdf_text(client: &reqwest::Client, page: &str, limits: &Limits) -> Result<Option<String>> {
    let pattern = Regex::new(
        r#"https://files\.speakerdeck\.com/presentations/[0-9a-f]{32}/[^"'<>\s]+\.pdf"#,
    )?;
    let Some(pdf_url) = pattern.find(page).map(|found| found.as_str().to_string()) else {
        return Ok(None);
    };

    let response = client.get(&pdf_url).send().await?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let bytes = response.bytes().await?;

    let extracted =
        tokio::task::spawn_blocking(move || pdf_extract::extract_text_from_mem(&bytes)).await?;
    let Ok(text) = extracted else {
        return Ok(None);
    };

    let normalized = normalize(&text);
    if normalized.chars().count() < limits.min_body_chars {
        return Ok(None);
    }
    Ok(Some(normalized))
}

fn sample(numbers: Vec<usize>, max_slides: usize) -> Vec<usize> {
    if numbers.len() <= max_slides || max_slides == 0 {
        return numbers;
    }
    let step = numbers.len() as f64 / max_slides as f64;
    (0..max_slides)
        .map(|index| numbers[(index as f64 * step) as usize])
        .collect()
}

async fn slide_images(client: &reqwest::Client, page: &str, max_slides: usize) -> Result<Content> {
    let id_pattern = Regex::new(r"presentations/([0-9a-f]{32})")?;
    let id = id_pattern
        .captures(page)
        .map(|caps| caps[1].to_string())
        .ok_or_else(|| anyhow!("スライドの識別子が見つかりません"))?;

    let player = get_text(client, &format!("https://speakerdeck.com/player/{id}")).await?;

    let slide_pattern = Regex::new(&format!(
        r"https://files\.speakerdeck\.com/presentations/{id}/slide_(\d+)\.jpg"
    ))?;
    let mut numbers: Vec<usize> = slide_pattern
        .captures_iter(&player)
        .filter_map(|caps| caps[1].parse().ok())
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    if numbers.is_empty() {
        return Err(anyhow!("スライドの画像が見つかりません"));
    }

    let truncated = numbers.len() > max_slides;
    let picked = sample(numbers, max_slides);

    let urls = picked
        .into_iter()
        .map(|number| {
            format!("https://files.speakerdeck.com/presentations/{id}/slide_{number}.jpg")
        })
        .collect();
    Ok(Content::Slides {
        urls,
        truncated,
        card: Card::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 上限以下の枚数は、そのまま全部返すこと
    #[test]
    fn keeps_all_when_within_limit() {
        assert_eq!(sample(vec![0, 1, 2], 10), vec![0, 1, 2]);
    }

    /// 上限を超えるときは、等間隔に間引いて上限ちょうどの枚数にすること
    #[test]
    fn thins_out_evenly_when_over_limit() {
        let picked = sample((0..10).collect(), 5);
        assert_eq!(picked, vec![0, 2, 4, 6, 8]);
    }

    /// 間引いても最初の1枚は必ず残ること
    #[test]
    fn always_keeps_first_slide() {
        let picked = sample((0..100).collect(), 7);
        assert_eq!(picked[0], 0);
        assert_eq!(picked.len(), 7);
    }
}
