//! Turning the search form's text fields into a [`SearchQuery`].

use chrono::{Days, NaiveDate, NaiveTime, TimeZone};

use super::model::{KindFilter, SearchQuery};
use super::modes::SearchMode;
use super::pattern::{Expr, PatternError};
use super::units::{parse_date, parse_size};

/// The search form as typed by the user.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchForm {
    /// Name pattern (Cathy syntax).
    pub pattern: String,
    /// Earliest modification date (see [`super::units::parse_date`]).
    pub from: String,
    /// Latest modification date, inclusive.
    pub to: String,
    /// Minimum size (see [`super::units::parse_size`]).
    pub min_size: String,
    /// Maximum size.
    pub max_size: String,
    /// Kinds of entries.
    pub kind: KindFilter,
    /// Search mode.
    pub mode: SearchMode,
}

/// The form field holding an invalid value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormError {
    /// Invalid pattern or expression.
    Pattern(PatternError),
    /// Invalid "from" date.
    From,
    /// Invalid "to" date.
    To,
    /// Invalid minimum size.
    MinSize,
    /// Invalid maximum size.
    MaxSize,
}

/// Validates the form. Dates are local days in `tz`: "from" starts at its midnight, "to" includes the whole day.
///
/// # Errors
/// The first invalid field.
pub fn parse_form<Tz: TimeZone>(form: &SearchForm, today: NaiveDate, tz: &Tz) -> Result<SearchQuery, FormError> {
    fn optional(text: &str) -> Option<&str> {
        Some(text.trim()).filter(|t| !t.is_empty())
    }
    let pattern = Expr::parse(&form.pattern).map_err(FormError::Pattern)?;
    let size = |text: &str, error| optional(text).map(|t| parse_size(t).map_err(|_| error)).transpose();
    let day = |text: &str, error, days_after: u64| {
        optional(text)
            .map(|t| {
                parse_date(t, today)
                    .ok()
                    .and_then(|d| d.checked_add_days(Days::new(days_after)))
                    .and_then(|d| tz.from_local_datetime(&d.and_time(NaiveTime::MIN)).earliest())
                    .map(|start| start.timestamp())
                    .ok_or(error)
            })
            .transpose()
    };
    Ok(SearchQuery {
        pattern,
        parents: form.mode == SearchMode::Parents,
        min_size: size(&form.min_size, FormError::MinSize)?,
        max_size: size(&form.max_size, FormError::MaxSize)?,
        from: day(&form.from, FormError::From, 0)?,
        // The "to" day is included: stop at the start of the next one.
        to: day(&form.to, FormError::To, 1)?,
        kind: form.kind,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, NaiveDate};

    use super::{FormError, SearchForm, parse_form};
    use crate::core::model::KindFilter;
    use crate::core::modes::SearchMode;
    use crate::core::pattern::PatternError;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 11).unwrap()
    }

    fn paris() -> FixedOffset {
        FixedOffset::east_opt(3600).unwrap()
    }

    #[test]
    fn empty_form_matches_all() {
        let q = parse_form(&SearchForm::default(), today(), &paris()).unwrap();
        assert!(q.pattern.is_empty());
        assert_eq!((q.min_size, q.max_size, q.from, q.to), (None, None, None, None));
        assert!(!q.parents);
    }

    #[test]
    fn fields_parsed() {
        let form = SearchForm {
            pattern: "asterix".to_owned(),
            from: "2026-11-10".to_owned(),
            to: "2026-11-10".to_owned(),
            min_size: "10M".to_owned(),
            max_size: " 1g ".to_owned(),
            kind: KindFilter::Files,
            mode: SearchMode::Parents,
        };
        let q = parse_form(&form, today(), &paris()).unwrap();
        // 2026-11-10 00:00 +01:00 = 2026-11-09 23:00 UTC.
        assert_eq!(q.from, Some(1_794_265_200));
        assert_eq!(q.to, Some(1_794_265_200 + 86_400));
        assert_eq!((q.min_size, q.max_size), (Some(10_000_000), Some(1_000_000_000)));
        assert_eq!(q.kind, KindFilter::Files);
        assert!(q.parents);
        assert!(q.pattern.matches("asterix"));
    }

    #[test]
    fn invalid_fields_named() {
        let bad = |f: fn(&mut SearchForm)| {
            let mut form = SearchForm::default();
            f(&mut form);
            parse_form(&form, today(), &paris()).unwrap_err()
        };
        assert_eq!(bad(|f| f.pattern = "a*b*c".to_owned()), FormError::Pattern(PatternError::TooManyWildcards));
        assert_eq!(bad(|f| f.pattern = "(a OR b".to_owned()), FormError::Pattern(PatternError::UnbalancedParentheses));
        assert_eq!(bad(|f| f.from = "yesterday".to_owned()), FormError::From);
        assert_eq!(bad(|f| f.to = "2026-02-30".to_owned()), FormError::To);
        assert_eq!(bad(|f| f.min_size = "big".to_owned()), FormError::MinSize);
        assert_eq!(bad(|f| f.max_size = "-1".to_owned()), FormError::MaxSize);
    }
}
