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
        println!("[{}] {}", $crate::clock::stamp(), format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        eprintln!("[{}] {}", $crate::clock::stamp(), format_args!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 止まっていた時間は、1時間以上なら「1時間23分」、1時間未満なら「5分」と書くこと
    #[test]
    fn writes_gap_in_japanese() {
        assert_eq!(gap_text(chrono::Duration::minutes(83)), "1時間23分");
        assert_eq!(gap_text(chrono::Duration::minutes(5)), "5分");
        assert_eq!(gap_text(chrono::Duration::minutes(120)), "2時間0分");
    }
}
