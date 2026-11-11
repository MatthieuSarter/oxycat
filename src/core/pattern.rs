//! Cathy-style search patterns: case- and accent-insensitive substring, `|` anchors, at most one inner `*`.

use super::fold::fold;

/// A parsed search pattern. Parts are folded; a single empty part matches everything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    /// The name must start with the first part.
    pub anchored_start: bool,
    /// The name must end with the last part.
    pub anchored_end: bool,
    /// One part, or two parts separated by the single allowed wildcard.
    pub parts: Vec<String>,
}

/// Why a pattern was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternError {
    /// More than one `*` inside the pattern.
    TooManyWildcards,
}

impl Pattern {
    /// Parses user input.
    ///
    /// Leading `|` anchors the start, trailing `|` the end; `*` at either end is ignored (`*str` ≡ `str*` ≡ `str`);
    /// a pattern with no text matches every name.
    ///
    /// # Errors
    /// [`PatternError::TooManyWildcards`] when the text contains more than one inner `*`.
    pub fn parse(input: &str) -> Result<Pattern, PatternError> {
        let mut text = input.trim();
        let anchored_start = text.starts_with('|');
        if anchored_start {
            text = &text[1..];
        }
        let anchored_end = text.ends_with('|');
        if anchored_end {
            text = &text[..text.len() - 1];
        }
        let parts: Vec<String> = text.trim_matches('*').split('*').map(fold).collect();
        if parts.len() > 2 {
            return Err(PatternError::TooManyWildcards);
        }
        if parts.iter().all(String::is_empty) {
            return Ok(Pattern { anchored_start: false, anchored_end: false, parts: vec![String::new()] });
        }
        Ok(Pattern { anchored_start, anchored_end, parts })
    }

    /// Tells whether the pattern matches every name.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parts.iter().all(String::is_empty)
    }

    /// Tells whether an already folded name matches.
    #[must_use]
    pub fn matches(&self, folded_name: &str) -> bool {
        let (first, second) = match self.parts.as_slice() {
            [only] => (only.as_str(), None),
            [a, b] => (a.as_str(), Some(b.as_str())),
            _ => return false,
        };
        let Some(second) = second else {
            return match (self.anchored_start, self.anchored_end) {
                (true, true) => folded_name == first,
                (true, false) => folded_name.starts_with(first),
                (false, true) => folded_name.ends_with(first),
                (false, false) => folded_name.contains(first),
            };
        };
        // The leftmost occurrence of the first part leaves the most room for the second one.
        let rest = if self.anchored_start {
            folded_name.strip_prefix(first)
        } else {
            folded_name.find(first).map(|i| &folded_name[i + first.len()..])
        };
        rest.is_some_and(|rest| if self.anchored_end { rest.ends_with(second) } else { rest.contains(second) })
    }

    /// Converts the pattern to a SQL `LIKE` expression, to be used with `ESCAPE '\'` on a folded column.
    #[must_use]
    pub fn to_like(&self) -> String {
        if self.is_empty() {
            return "%".to_owned();
        }
        let body = self.parts.iter().map(|p| escape_like(p)).collect::<Vec<_>>().join("%");
        let start = if self.anchored_start { "" } else { "%" };
        let end = if self.anchored_end { "" } else { "%" };
        format!("{start}{body}{end}")
    }
}

/// Escapes the `LIKE` metacharacters `\`, `%` and `_` with a backslash.
fn escape_like(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Pattern, PatternError};
    use crate::core::fold::fold;

    fn p(s: &str) -> Pattern {
        Pattern::parse(s).unwrap()
    }

    #[test]
    fn ext_anchor() {
        assert!(p(".c|").matches("main.c"));
        assert!(!p(".c|").matches("main.cpp"));
    }

    #[test]
    fn both_anchors_wildcard() {
        let pat = p("|s*doc|");
        assert!(pat.matches("solution.doc"));
        assert!(pat.matches("stringdoc"));
        assert!(!pat.matches("soldocument.txt"));
    }

    #[test]
    fn start_anchor_only() {
        let pat = p("|s*doc");
        assert!(pat.matches("soldocument.txt"));
        assert!(!pat.matches("unsoldocument.txt"));
    }

    #[test]
    fn unanchored_wildcard() {
        assert!(p("s*doc").matches("unsoldocument.txt"));
        assert!(!p("s*doc").matches("document"));
    }

    #[test]
    fn wildcard_parts_do_not_overlap() {
        assert!(!p("|ab*bc|").matches("abc"));
        assert!(p("|ab*bc|").matches("abbc"));
    }

    #[test]
    fn edge_stars_equivalent() {
        for s in ["*str", "str*", "str"] {
            assert_eq!(p(s), p("str"));
        }
    }

    #[test]
    fn accents_folded() {
        assert!(p("ASTERIX").matches(&fold("Astérix 01.cbz")));
        assert!(p("astérix").matches("asterix"));
    }

    #[test]
    fn two_inner_stars_rejected() {
        assert_eq!(Pattern::parse("a*b*c"), Err(PatternError::TooManyWildcards));
    }

    #[test]
    fn empty_and_bars_match_all() {
        for s in ["", "   ", "|", "||", "*"] {
            assert!(p(s).matches("anything"), "{s:?}");
        }
    }

    #[test]
    fn like_escapes_metachars() {
        assert_eq!(p("100%").to_like(), r"%100\%%");
        assert_eq!(p("|my_file").to_like(), r"my\_file%");
        assert_eq!(p("|s*doc|").to_like(), "s%doc");
        assert_eq!(p(r"a\b").to_like(), r"%a\\b%");
        assert_eq!(p("").to_like(), "%");
    }
}
