use anyhow::{bail, Context, Result};
use chrono::NaiveDate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatePattern {
    pattern: String,
    chrono_format: String,
}

impl DatePattern {
    pub fn new(pattern: &str) -> Result<Self> {
        let mut chrono_format = String::new();
        let mut chars = pattern.chars().peekable();

        while let Some(c) = chars.next() {
            if !c.is_ascii_alphabetic() {
                match c {
                    '%' => chrono_format.push_str("%%"),
                    _ => chrono_format.push(c),
                }
                continue;
            }

            let mut count = 1;
            while chars.peek() == Some(&c) {
                chars.next();
                count += 1;
            }

            let specifier = match (c, count) {
                ('y', 2) => "%y",
                ('y', _) => "%Y",
                ('M', 1) => "%-m",
                ('M', 2) => "%m",
                ('M', 3) => "%b",
                ('M', _) => "%B",
                ('d', 1) => "%-d",
                ('d', _) => "%d",
                ('E', 1..=3) => "%a",
                ('E', _) => "%A",
                _ => bail!(
                    "Unsupported letter {:?} in date pattern {:?}. Supported letters are y, M, d and E.",
                    c,
                    pattern
                ),
            };
            chrono_format.push_str(specifier);
        }

        Ok(DatePattern {
            pattern: pattern.to_string(),
            chrono_format,
        })
    }

    pub fn parse(&self, value: &str) -> Result<NaiveDate> {
        NaiveDate::parse_from_str(value.trim(), &self.chrono_format).with_context(|| {
            format!(
                "Date {:?} does not match the date pattern {:?}",
                value, self.pattern
            )
        })
    }

    pub fn format(&self, date: NaiveDate) -> String {
        date.format(&self.chrono_format).to_string()
    }
}

#[cfg(test)]
mod test {
    use crate::dates::*;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    #[test]
    fn test_parse_griffin_input_pattern() {
        let pattern = DatePattern::new("yyyy MM dd").unwrap();

        assert_eq!(date(2015, 6, 25), pattern.parse("2015 06 25").unwrap());
        assert_eq!(date(2015, 6, 5), pattern.parse(" 2015 06 05 ").unwrap());
    }

    #[test]
    fn test_parse_rejects_mismatched_date() {
        let pattern = DatePattern::new("yyyy MM dd").unwrap();

        let error = pattern.parse("2015-06-25").unwrap_err();

        assert!(
            format!("{:#}", error).contains("does not match the date pattern \"yyyy MM dd\""),
            "{:#}",
            error
        );
    }

    #[test]
    fn test_format_griffin_output_pattern() {
        let pattern = DatePattern::new("MMM d yyyy").unwrap();

        assert_eq!("Jun 5 2015", pattern.format(date(2015, 6, 5)));
    }

    #[test]
    fn test_pattern_conversions() {
        let cases = [
            ("yyyy-MM-dd", date(2024, 1, 9), "2024-01-09"),
            ("yy/M/d", date(2024, 1, 9), "24/1/9"),
            ("MMMM d, yyyy", date(2024, 1, 9), "January 9, 2024"),
            ("EEE, dd MMM yyyy", date(2024, 1, 9), "Tue, 09 Jan 2024"),
            ("EEEE yyyy MM dd", date(2024, 1, 9), "Tuesday 2024 01 09"),
            ("yyyy MM dd%", date(2024, 1, 9), "2024 01 09%"),
        ];

        for (pattern, value, expected) in cases {
            let pattern = DatePattern::new(pattern).unwrap();
            assert_eq!(expected, pattern.format(value));
            assert_eq!(value, pattern.parse(expected).unwrap());
        }
    }

    #[test]
    fn test_unsupported_pattern_letter() {
        let error = DatePattern::new("yyyy HH:mm").unwrap_err();

        assert!(
            format!("{:#}", error).contains("Unsupported letter 'H'"),
            "{:#}",
            error
        );
    }
}
