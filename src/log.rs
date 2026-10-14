use chrono::{FixedOffset, Utc};

const JST_OFFSET_SECONDS: i32 = 9 * 3600;

pub fn stamp() -> String {
    match FixedOffset::east_opt(JST_OFFSET_SECONDS) {
        Some(jst) => Utc::now()
            .with_timezone(&jst)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        None => Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    }
}

pub fn gap_text(gap: chrono::Duration) -> String {
    let minutes = gap.num_minutes();
    if minutes >= 60 {
        format!("{}時間{}分", minutes / 60, minutes % 60)
    } else {
        format!("{minutes}分")
    }
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        println!("[{}] {}", $crate::log::stamp(), format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        eprintln!("[{}] {}", $crate::log::stamp(), format_args!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// 止まっていた時間は、1時間以上なら「1時間23分」、1時間未満なら「5分」と書くこと
    #[test]
    fn writes_gap_in_japanese() {
        assert_eq!(gap_text(chrono::Duration::minutes(83)), "1時間23分");
        assert_eq!(gap_text(chrono::Duration::minutes(5)), "5分");
        assert_eq!(gap_text(chrono::Duration::minutes(120)), "2時間0分");
    }
}
