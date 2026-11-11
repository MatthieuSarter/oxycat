//! Case and accent folding used for accent-insensitive search.

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Folds a name for comparison: lowercase, without accents.
///
/// Lowercases first, then decomposes (NFD) and drops combining marks, so `"Astérix"` becomes `"asterix"`.
#[must_use]
pub fn fold(s: &str) -> String {
    s.to_lowercase().nfd().filter(|c| !is_combining_mark(*c)).collect()
}

#[cfg(test)]
mod tests {
    use super::fold;

    #[test]
    fn fold_strips_accents_and_case() {
        assert_eq!(fold("Astérix ÉTÉ"), "asterix ete");
    }

    #[test]
    fn fold_keeps_plain_ascii() {
        assert_eq!(fold("my_file-01.cbz"), "my_file-01.cbz");
    }
}
