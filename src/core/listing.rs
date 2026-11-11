//! Pure helpers behind the interface lists: filtering, sorting, totals, clipboard and tree export, date display.

use std::cmp::Ordering;
use std::fmt::{Display, Write};

use chrono::TimeZone;

use super::fold::fold;
use super::model::{Entry, Hit};
use crate::catalog::store::CatalogInfo;

/// Line ending of exported text: the platform's own, so pasted or saved text looks native.
pub const NEWLINE: &str = if cfg!(windows) { "\r\n" } else { "\n" };

/// Catalogs whose name or comment contains `filter` (case- and accent-insensitive); all when it is blank.
#[must_use]
pub fn filter_catalogs<'a>(cats: &'a [CatalogInfo], filter: &str) -> Vec<&'a CatalogInfo> {
    let needle = fold(filter.trim());
    cats.iter()
        .filter(|c| fold(&c.volume.name).contains(&needle) || fold(&c.volume.comment).contains(&needle))
        .collect()
}

/// Columns a list can be sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    /// Entry name.
    #[default]
    Name,
    /// Size (folders count as smaller than any file).
    Size,
    /// Modification date.
    Date,
    /// Parent folder path.
    Path,
    /// Catalog name.
    Catalog,
}

/// Sorts search hits by a column; ties are broken by name.
pub fn sort_hits(hits: &mut [Hit], key: SortKey, ascending: bool) {
    hits.sort_by(|a, b| {
        let primary = match key {
            SortKey::Name => Ordering::Equal,
            SortKey::Size => a.entry.size.cmp(&b.entry.size),
            SortKey::Date => a.entry.mtime.cmp(&b.entry.mtime),
            SortKey::Path => a.path.to_lowercase().cmp(&b.path.to_lowercase()),
            SortKey::Catalog => a.catalog_name.to_lowercase().cmp(&b.catalog_name.to_lowercase()),
        };
        let by_name = a.name_fold.cmp(&b.name_fold).then_with(|| a.entry.name.cmp(&b.entry.name));
        directed(primary.then(if key == SortKey::Name { by_name } else { Ordering::Equal }), ascending).then(by_name)
    });
}

/// Sorts folder entries by a column, folders always first; ties are broken by name.
pub fn sort_entries(entries: &mut [Entry], key: SortKey, ascending: bool) {
    entries.sort_by_cached_key(|e| (!e.is_dir, fold(&e.name)));
    entries.sort_by(|a, b| {
        let primary = match key {
            SortKey::Size => a.size.cmp(&b.size),
            SortKey::Date => a.mtime.cmp(&b.mtime),
            SortKey::Name | SortKey::Path | SortKey::Catalog => Ordering::Equal,
        };
        let by_name =
            if key == SortKey::Name { directed(fold(&a.name).cmp(&fold(&b.name)), ascending) } else { Ordering::Equal };
        b.is_dir.cmp(&a.is_dir).then(directed(primary, ascending)).then(by_name)
    });
}

/// Reverses an ordering for a descending sort.
fn directed(ordering: Ordering, ascending: bool) -> Ordering {
    if ascending { ordering } else { ordering.reverse() }
}

/// Number of selected rows and their total size (folders count as 0).
#[must_use]
pub fn selection_total(sizes: impl Iterator<Item = Option<u64>>) -> (usize, u64) {
    sizes.fold((0, 0), |(n, total), size| (n + 1, total + size.unwrap_or(0)))
}

/// Tab-separated text with a header row, ready to paste into a spreadsheet. Tabs and line breaks inside cells become
/// spaces.
#[must_use]
pub fn rows_to_tsv(header: &[&str], rows: &[Vec<String>]) -> String {
    let clean = |cell: &str| cell.replace(['\t', '\r', '\n'], " ");
    let mut out = header.join("\t") + NEWLINE;
    for row in rows {
        out += &row.iter().map(|c| clean(c)).collect::<Vec<_>>().join("\t");
        out += NEWLINE;
    }
    out
}

/// Text tree of folders: the root on the first line, then one folder per line indented by two spaces per level.
#[must_use]
pub fn tree_text(root: &str, dirs: &[(usize, String)]) -> String {
    let mut out = format!("{root}{NEWLINE}");
    for (depth, name) in dirs {
        let _ = write!(out, "{}{name}{NEWLINE}", "  ".repeat(depth + 1));
    }
    out
}

/// Formats a UTC Unix timestamp in a time zone with a `chrono` format; empty when out of range or when the format is
/// invalid (it may come from the settings file).
#[must_use]
pub fn format_timestamp<Tz: TimeZone>(ts: i64, fmt: &str, tz: &Tz) -> String
where
    Tz::Offset: Display,
{
    let Some(time) = tz.timestamp_opt(ts, 0).single() else {
        return String::new();
    };
    let mut out = String::new();
    if write!(out, "{}", time.format(fmt)).is_err() {
        return String::new();
    }
    out
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, Utc};

    use super::{
        NEWLINE, SortKey, filter_catalogs, format_timestamp, rows_to_tsv, selection_total, sort_entries, sort_hits,
        tree_text,
    };
    use crate::catalog::store::CatalogInfo;
    use crate::catalog::store::tests::sample_volume;
    use crate::core::model::{Entry, Hit};
    use crate::core::modes::tests::hit;

    fn cat(name: &str, comment: &str) -> CatalogInfo {
        let mut volume = sample_volume(name);
        volume.comment = comment.to_owned();
        CatalogInfo { file: format!("{name}.db").into(), volume, modified: std::time::SystemTime::UNIX_EPOCH }
    }

    #[test]
    fn filter_by_name_or_comment() {
        let cats = [cat("B1", "Bandes dessinées"), cat("B2", "Films"), cat("sarterm", "")];
        let names = |f: &str| filter_catalogs(&cats, f).iter().map(|c| c.volume.name.clone()).collect::<Vec<_>>();
        assert_eq!(names("b"), ["B1", "B2"]);
        assert_eq!(names("DESSINEE"), ["B1"]);
        assert_eq!(names("film"), ["B2"]);
        assert_eq!(names("  "), ["B1", "B2", "sarterm"]);
    }

    fn hits() -> Vec<Hit> {
        let mut a = hit("Z", false, "beta.txt", Some(5));
        a.path = "\\b".to_owned();
        a.entry.mtime = Some(30);
        let mut b = hit("A", false, "Alpha.txt", Some(50));
        b.path = "\\c".to_owned();
        b.entry.mtime = Some(10);
        let mut c = hit("M", false, "dir", None);
        c.path = "\\a".to_owned();
        c.entry.mtime = Some(20);
        vec![a, b, c]
    }

    fn order(hits: &[Hit]) -> Vec<&str> {
        hits.iter().map(|h| h.entry.name.as_str()).collect()
    }

    #[test]
    fn sort_hits_each_key() {
        let mut h = hits();
        sort_hits(&mut h, SortKey::Name, true);
        assert_eq!(order(&h), ["Alpha.txt", "beta.txt", "dir"]);
        sort_hits(&mut h, SortKey::Name, false);
        assert_eq!(order(&h), ["dir", "beta.txt", "Alpha.txt"]);
        sort_hits(&mut h, SortKey::Size, true);
        assert_eq!(order(&h), ["dir", "beta.txt", "Alpha.txt"]);
        sort_hits(&mut h, SortKey::Date, true);
        assert_eq!(order(&h), ["Alpha.txt", "dir", "beta.txt"]);
        sort_hits(&mut h, SortKey::Path, true);
        assert_eq!(order(&h), ["dir", "beta.txt", "Alpha.txt"]);
        sort_hits(&mut h, SortKey::Catalog, false);
        assert_eq!(order(&h), ["beta.txt", "dir", "Alpha.txt"]);
    }

    #[test]
    fn sort_entries_folders_first() {
        let e = |name: &str, size: Option<u64>| Entry {
            id: 0,
            parent_id: None,
            name: name.to_owned(),
            is_dir: size.is_none(),
            size,
            mtime: None,
        };
        let mut entries = vec![e("b.txt", Some(1)), e("Z", None), e("a.txt", Some(9)), e("A", None)];
        sort_entries(&mut entries, SortKey::Size, false);
        let names: Vec<&str> = entries.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["A", "Z", "a.txt", "b.txt"]);
        sort_entries(&mut entries, SortKey::Name, false);
        let names: Vec<&str> = entries.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["Z", "A", "b.txt", "a.txt"]);
    }

    #[test]
    fn selection_totals() {
        assert_eq!(selection_total([Some(3), None, Some(4)].into_iter()), (3, 7));
        assert_eq!(selection_total(std::iter::empty()), (0, 0));
    }

    #[test]
    fn tsv_rows() {
        let rows = vec![vec!["a\tb".to_owned(), "1".to_owned()], vec!["line\nbreak".to_owned(), "2".to_owned()]];
        assert_eq!(
            rows_to_tsv(&["Name", "Size"], &rows),
            format!("Name\tSize{NEWLINE}a b\t1{NEWLINE}line break\t2{NEWLINE}")
        );
    }

    #[test]
    fn tree_indentation() {
        let dirs = vec![(0, "A".to_owned()), (1, "B".to_owned()), (0, "C".to_owned())];
        assert_eq!(tree_text("B1:\\BD", &dirs), format!("B1:\\BD{NEWLINE}  A{NEWLINE}    B{NEWLINE}  C{NEWLINE}"));
    }

    #[test]
    fn timestamps() {
        assert_eq!(format_timestamp(0, "%Y-%m-%d %H:%M", &Utc), "1970-01-01 00:00");
        let paris = FixedOffset::east_opt(3600).unwrap();
        assert_eq!(format_timestamp(0, "%d/%m/%Y %H:%M", &paris), "01/01/1970 01:00");
        assert_eq!(format_timestamp(i64::MAX, "%Y", &Utc), "");
        assert_eq!(format_timestamp(0, "%Q bad", &Utc), "");
    }
}
