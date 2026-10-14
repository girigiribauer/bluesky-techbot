pub fn normalize(url: &str) -> String {
    let unwrapped = unwrap_archive(url);
    let without_fragment = unwrapped.split('#').next().unwrap_or(&unwrapped);
    strip_tracking(without_fragment)
}

fn unwrap_archive(url: &str) -> String {
    let Some(rest) = url.strip_prefix("https://web.archive.org/web/") else {
        return url.to_string();
    };
    match rest.find("http") {
        Some(index) => rest[index..].to_string(),
        None => url.to_string(),
    }
}

fn strip_tracking(url: &str) -> String {
    let Some((base, query)) = url.split_once('?') else {
        return url.to_string();
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            let name = pair.split('=').next().unwrap_or("");
            !(name.starts_with("utm_")
                || matches!(name, "ref" | "ref_src" | "fbclid" | "gclid" | "spm" | ""))
        })
        .collect();
    if kept.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", kept.join("&"))
    }
}

pub fn absolutize(base: &str, link: &str) -> Option<String> {
    let link = link.trim();
    if link.is_empty() {
        return None;
    }
    if link.starts_with("http://") || link.starts_with("https://") {
        return Some(link.to_string());
    }
    let scheme_end = base.find("://")? + 3;
    let host_end = base[scheme_end..]
        .find('/')
        .map(|index| scheme_end + index)
        .unwrap_or(base.len());

    if let Some(rest) = link.strip_prefix("//") {
        return Some(format!("{}//{rest}", &base[..scheme_end - 2]));
    }
    if link.starts_with('/') {
        return Some(format!("{}{link}", &base[..host_end]));
    }
    let directory = base[..base.rfind('/').unwrap_or(host_end)].to_string();
    Some(format!("{directory}/{link}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// web.archive.org 経由のURLから、元の記事のURLを取り出すこと
    #[test]
    fn unwraps_archive_url() {
        assert_eq!(
            normalize("https://web.archive.org/web/20261011101825/https://example.com/a"),
            "https://example.com/a"
        );
    }

    /// 流入経路を示すだけの引数を取り除くこと
    #[test]
    fn strips_tracking_parameters() {
        assert_eq!(
            normalize("https://example.com/a?utm_source=x&id=3&fbclid=y"),
            "https://example.com/a?id=3"
        );
        assert_eq!(
            normalize("https://example.com/a?utm_source=x"),
            "https://example.com/a"
        );
    }

    /// ページ内の位置を示す記号を取り除くこと
    #[test]
    fn strips_fragment() {
        assert_eq!(
            normalize("https://example.com/a#section"),
            "https://example.com/a"
        );
    }

    /// 絶対URLはそのまま、相対URLは元のURLを基準に組み立てること
    #[test]
    fn absolutizes_links() {
        let base = "https://example.com/blog/entry";

        assert_eq!(
            absolutize(base, "https://cdn.example.com/a.png").as_deref(),
            Some("https://cdn.example.com/a.png")
        );
        assert_eq!(
            absolutize(base, "/ogp.png").as_deref(),
            Some("https://example.com/ogp.png")
        );
        assert_eq!(
            absolutize(base, "//cdn.example.com/a.png").as_deref(),
            Some("https://cdn.example.com/a.png")
        );
        assert_eq!(
            absolutize(base, "a.png").as_deref(),
            Some("https://example.com/blog/a.png")
        );
    }

    /// 空のリンクは無しとして扱うこと
    #[test]
    fn rejects_empty_link() {
        assert!(absolutize("https://example.com/a", "  ").is_none());
    }

    /// 取り除くものが無いURLは、そのままであること
    #[test]
    fn keeps_plain_url() {
        assert_eq!(
            normalize("https://example.com/a?id=3"),
            "https://example.com/a?id=3"
        );
    }
}
