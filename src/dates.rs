use std::time::{SystemTime, UNIX_EPOCH};

use objc2::rc::Retained;
use objc2_foundation::{NSDate, NSDateFormatter, NSDateFormatterStyle};

/// Dates as the system shows them: user's language and region, with
/// "Today" / "Yesterday" for recent ones.
pub struct DateFormat(Retained<NSDateFormatter>);

impl DateFormat {
    pub fn new() -> Self {
        let f = NSDateFormatter::new();
        f.setDateStyle(NSDateFormatterStyle::MediumStyle);
        f.setTimeStyle(NSDateFormatterStyle::ShortStyle);
        f.setDoesRelativeDateFormatting(true);
        Self(f)
    }

    pub fn format(&self, time: SystemTime) -> String {
        let secs = match time.duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_secs_f64(),
            Err(e) => -e.duration().as_secs_f64(),
        };
        self.0
            .stringFromDate(&NSDate::dateWithTimeIntervalSince1970(secs))
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_a_date_with_time() {
        let f = DateFormat::new();
        let text = f.format(UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000));
        println!("{text}");
        assert!(text.contains("2001"), "{text}");
        assert!(!f.format(SystemTime::now()).is_empty());
    }
}
