//! Mapping of catalog names to file names.

/// Turns a catalog name into a file stem valid on Windows and Linux.
///
/// Replaces `<>:"/\|?*` and control characters with `_`, trims trailing spaces and dots, prefixes Windows reserved
/// device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`, any case) with `_`, and maps an empty
/// result to `_`.
#[must_use]
pub fn catalog_file_stem(name: &str) -> String {
    let replaced: String =
        name.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c }).collect();
    let stem = replaced.trim_end_matches([' ', '.']).trim_start();
    if stem.is_empty() {
        return "_".to_owned();
    }
    let upper = stem.to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && matches!(upper.as_bytes()[3], b'1'..=b'9'));
    if reserved { format!("_{stem}") } else { stem.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::catalog_file_stem;

    #[test]
    fn stems() {
        assert_eq!(catalog_file_stem("B1"), "B1");
        assert_eq!(catalog_file_stem("a:b?"), "a_b_");
        assert_eq!(catalog_file_stem("x<>\"/\\|*y"), "x_______y");
        assert_eq!(catalog_file_stem("tab\there"), "tab_here");
        assert_eq!(catalog_file_stem("con"), "_con");
        assert_eq!(catalog_file_stem("LPT9"), "_LPT9");
        assert_eq!(catalog_file_stem("COM10"), "COM10");
        assert_eq!(catalog_file_stem("disk. "), "disk");
        assert_eq!(catalog_file_stem(""), "_");
        assert_eq!(catalog_file_stem(" . "), "_");
        assert_eq!(catalog_file_stem("Astérix"), "Astérix");
    }
}
