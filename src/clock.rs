use chrono::{DateTime, Duration, FixedOffset, SecondsFormat, Utc};

const JST_OFFSET_SECONDS: i32 = 9 * 3600;

pub fn now_text() -> String {
    utc_text(Utc::now())
}

pub fn ago_text(span: Duration) -> String {
    utc_text(Utc::now() - span)
}

fn utc_text(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Micros, false)
}

pub fn stamp() -> String {
    match FixedOffset::east_opt(JST_OFFSET_SECONDS) {
        Some(jst) => Utc::now()
            .with_timezone(&jst)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        None => Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    }
}

pub fn jst_datetime(stored: &str) -> String {
    format_jst(stored, "%Y-%m-%d %H:%M")
        .unwrap_or_else(|| stored.get(..16).unwrap_or(stored).replace('T', " "))
}

pub fn jst_date(stored: &str) -> String {
    format_jst(stored, "%Y-%m-%d").unwrap_or_else(|| stored.get(..10).unwrap_or(stored).to_string())
}

fn format_jst(stored: &str, format: &str) -> Option<String> {
    let parsed = DateTime::parse_from_rfc3339(stored).ok()?;
    let jst = FixedOffset::east_opt(JST_OFFSET_SECONDS)?;
    Some(parsed.with_timezone(&jst).format(format).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 保存する時刻は、秒より下をマイクロ秒6桁に固定し、末尾を +00:00 と書くこと
    #[test]
    fn writes_stored_time_in_one_shape() {
        let at = DateTime::parse_from_rfc3339("2026-10-14T05:22:40Z")
            .unwrap()
            .with_timezone(&Utc);

        assert_eq!(utc_text(at), "2026-10-14T05:22:40.000000+00:00");
    }

    /// 桁が固定されているので、文字列のまま比べても時刻の前後と一致すること
    #[test]
    fn compares_stored_times_as_text() {
        let earlier = utc_text(
            DateTime::parse_from_rfc3339("2026-10-14T05:22:40.983425Z")
                .unwrap()
                .with_timezone(&Utc),
        );
        let later = utc_text(
            DateTime::parse_from_rfc3339("2026-10-14T05:22:41Z")
                .unwrap()
                .with_timezone(&Utc),
        );

        assert!(earlier < later);
        assert_eq!(earlier.len(), later.len());
    }

    /// 今の時刻も、さかのぼった時刻も、同じ形で書くこと
    #[test]
    fn writes_now_and_past_in_the_same_shape() {
        let now = now_text();
        let past = ago_text(Duration::hours(24));

        assert_eq!(now.len(), past.len());
        assert!(past < now);
        assert!(now.ends_with("+00:00"));
    }

    /// 日時は「年-月-日 時:分:秒」の形で、日本時間で出ること
    #[test]
    fn stamps_in_japan_time() {
        let stamp = stamp();
        let jst = FixedOffset::east_opt(JST_OFFSET_SECONDS).unwrap();
        let expected = Utc::now()
            .with_timezone(&jst)
            .format("%Y-%m-%d %H")
            .to_string();

        assert_eq!(stamp.len(), "2026-10-14 02:33:44".len());
        assert!(stamp.starts_with(&expected));
    }

    /// 保存してある UTC の時刻を、9時間進めた日本時間の「年-月-日 時:分」で出すこと
    #[test]
    fn shows_stored_time_in_japan_time() {
        assert_eq!(
            jst_datetime("2026-10-18T21:05:12.345678+00:00"),
            "2026-10-19 06:05"
        );
    }

    /// 日付だけのときも日本時間で数えること（UTC の夕方は日本では翌日）
    #[test]
    fn shows_stored_date_in_japan_time() {
        assert_eq!(jst_date("2026-10-18T21:05:12.345678+00:00"), "2026-10-19");
        assert_eq!(jst_date("2026-10-18T14:39:32.563114+00:00"), "2026-10-18");
    }

    /// 日時として読めない文字列が来ても、画面を壊さず元の形のまま出すこと
    #[test]
    fn keeps_unreadable_time_as_is() {
        assert_eq!(jst_datetime("2026-10-18 21:05:12"), "2026-10-18 21:05");
        assert_eq!(jst_date("2026-10-18 21:05:12"), "2026-10-18");
        assert_eq!(jst_datetime(""), "");
    }
}
