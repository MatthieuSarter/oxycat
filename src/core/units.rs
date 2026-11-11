//! Parsing of user-entered sizes and dates, and locale-aware formatting of numbers and sizes.
//!
//! Size units are decimal, as in Cathy: `K` = 10³, `M` = 10⁶, `G` = 10⁹, `T` = 10¹².

use chrono::NaiveDate;

/// A user input that could not be parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    /// The text is not a valid value.
    Invalid,
}

/// How numbers and sizes are written in a given language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumberFormat {
    /// Thousands separator.
    pub thousands: std::borrow::Cow<'static, str>,
    /// Decimal separator.
    pub decimal: char,
    /// Unit names for bytes, kilo, mega, giga and tera.
    pub units: [&'static str; 5],
}

/// Parses a size such as `3M`, `3000k` or `3000000` (decimal units, any case). Empty input is the caller's concern.
///
/// # Errors
/// [`InputError::Invalid`] for anything else than digits followed by an optional `K`, `M`, `G` or `T`.
pub fn parse_size(input: &str) -> Result<u64, InputError> {
    let text = input.trim();
    let (digits, multiplier) = match text.chars().last().map(|c| c.to_ascii_uppercase()) {
        Some('K') => (&text[..text.len() - 1], 1_000),
        Some('M') => (&text[..text.len() - 1], 1_000_000),
        Some('G') => (&text[..text.len() - 1], 1_000_000_000),
        Some('T') => (&text[..text.len() - 1], 1_000_000_000_000),
        _ => (text, 1),
    };
    parse_digits(digits).and_then(|n| n.checked_mul(multiplier)).ok_or(InputError::Invalid)
}

/// Parses a non-empty run of ASCII digits, without sign, overflow-checked.
fn parse_digits(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// Parses a date: `YYYY-MM-DD`, `YYYY-MM` (first of the month), `YYYY` (January 1st) or `-n` (n days before `today`).
///
/// # Errors
/// [`InputError::Invalid`] when the text matches none of these forms or names an impossible date.
pub fn parse_date(input: &str, today: NaiveDate) -> Result<NaiveDate, InputError> {
    let text = input.trim();
    if let Some(days) = text.strip_prefix('-') {
        return parse_digits(days)
            .and_then(|n| today.checked_sub_days(chrono::Days::new(n)))
            .ok_or(InputError::Invalid);
    }
    let fields: Vec<&str> = text.split('-').collect();
    let number = |s: &str, len: std::ops::RangeInclusive<usize>| {
        if len.contains(&s.len()) { parse_digits(s).and_then(|n| u32::try_from(n).ok()) } else { None }
    };
    let year = fields.first().and_then(|y| number(y, 4..=4)).and_then(|y| i32::try_from(y).ok());
    let month = fields.get(1).map_or(Some(1), |m| number(m, 1..=2));
    let day = fields.get(2).map_or(Some(1), |d| number(d, 1..=2));
    match (year, month, day) {
        (Some(y), Some(m), Some(d)) if fields.len() <= 3 => NaiveDate::from_ymd_opt(y, m, d).ok_or(InputError::Invalid),
        _ => Err(InputError::Invalid),
    }
}

/// Formats an integer with the language's thousands separator.
#[must_use]
pub fn format_int(n: u64, nf: &NumberFormat) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() * 2);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push_str(&nf.thousands);
        }
        out.push(c);
    }
    out
}

/// Formats a byte count with the largest decimal unit whose value is at least 1: one decimal below 100, none above.
#[must_use]
pub fn format_size(bytes: u64, nf: &NumberFormat) -> String {
    let mut unit = 0;
    let mut divisor: u128 = 1;
    while unit < 4 && u128::from(bytes) >= divisor * 1000 {
        unit += 1;
        divisor *= 1000;
    }
    let name = nf.units[unit];
    if unit == 0 {
        return format!("{} {name}", format_int(bytes, nf));
    }
    // Integer arithmetic only: tenths of the unit, rounded half up.
    let tenths = (u128::from(bytes) * 10 + divisor / 2) / divisor;
    if tenths >= 1000 {
        let whole = u64::try_from((u128::from(bytes) + divisor / 2) / divisor).unwrap_or(u64::MAX);
        format!("{} {name}", format_int(whole, nf))
    } else {
        format!("{}{}{} {name}", tenths / 10, nf.decimal, tenths % 10)
    }
}

#[cfg(test)]
mod tests {
    use super::{InputError, NumberFormat, format_int, format_size, parse_date, parse_size};
    use chrono::NaiveDate;

    const FR: NumberFormat = NumberFormat {
        thousands: std::borrow::Cow::Borrowed("\u{202F}"),
        decimal: ',',
        units: ["o", "Ko", "Mo", "Go", "To"],
    };
    const EN: NumberFormat =
        NumberFormat { thousands: std::borrow::Cow::Borrowed(","), decimal: '.', units: ["B", "KB", "MB", "GB", "TB"] };

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn sizes_cathy_equivalents() {
        for s in ["3M", "3m", "3000K", "3000k", "3000000", " 3M "] {
            assert_eq!(parse_size(s), Ok(3_000_000), "{s:?}");
        }
        assert_eq!(parse_size("3G"), Ok(3_000_000_000));
        assert_eq!(parse_size("2t"), Ok(2_000_000_000_000));
        assert_eq!(parse_size("0"), Ok(0));
    }

    #[test]
    fn sizes_invalid() {
        for s in ["abc", "-5", "3X", "M", "", "1.5G", "99999999999999999999"] {
            assert_eq!(parse_size(s), Err(InputError::Invalid), "{s:?}");
        }
    }

    #[test]
    fn dates_all_forms() {
        let today = d(2026, 11, 11);
        assert_eq!(parse_date("2019-03-02", today), Ok(d(2019, 3, 2)));
        assert_eq!(parse_date("2019-03", today), Ok(d(2019, 3, 1)));
        assert_eq!(parse_date("2019", today), Ok(d(2019, 1, 1)));
        assert_eq!(parse_date("-10", today), Ok(d(2026, 11, 1)));
        assert_eq!(parse_date(" -0 ", today), Ok(today));
    }

    #[test]
    fn dates_invalid() {
        let today = d(2026, 11, 11);
        for s in ["2019-13-01", "2019-02-30", "19", "abc", "", "-x", "2019-3-1x"] {
            assert_eq!(parse_date(s, today), Err(InputError::Invalid), "{s:?}");
        }
    }

    #[test]
    fn ints_grouped() {
        assert_eq!(format_int(214_532, &FR), "214\u{202F}532");
        assert_eq!(format_int(1_234_567, &EN), "1,234,567");
        assert_eq!(format_int(999, &EN), "999");
        assert_eq!(format_int(0, &EN), "0");
    }

    #[test]
    fn sizes_formatted() {
        assert_eq!(format_size(48_200_000, &FR), "48,2 Mo");
        assert_eq!(format_size(512, &FR), "512 o");
        assert_eq!(format_size(407_000_000_000, &FR), "407 Go");
        assert_eq!(format_size(1_500_000_000_000, &EN), "1.5 TB");
        assert_eq!(format_size(2_000_000_000_000_000, &EN), "2,000 TB");
        assert_eq!(format_size(1000, &EN), "1.0 KB");
    }
}
