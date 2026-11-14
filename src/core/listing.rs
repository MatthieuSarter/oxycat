//! Pure helpers behind the interface lists: filtering, sorting, totals, clipboard and tree export, date display.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Write};
use std::path::PathBuf;

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

/// One row of the search results: hits identical but for their volume, sorted by catalog name; never empty.
pub type HitRow = Vec<Hit>;

/// Rows of the search results, in the order of `hits`: one per hit, or with `group`, one per set of hits sharing name,
/// size, modification date and folder path (the same entry found on several volumes).
#[must_use]
pub fn hit_rows(hits: impl IntoIterator<Item = Hit>, group: bool) -> Vec<HitRow> {
    let mut rows: Vec<HitRow> = Vec::new();
    let mut index: HashMap<(String, Option<u64>, Option<i64>, String), usize> = HashMap::new();
    for hit in hits {
        if !group {
            rows.push(vec![hit]);
            continue;
        }
        let key = (hit.entry.name.clone(), hit.entry.size, hit.entry.mtime, hit.path.clone());
        let next = rows.len();
        let i = *index.entry(key).or_insert(next);
        if i == next {
            rows.push(Vec::new());
        }
        rows[i].push(hit);
    }
    for row in &mut rows {
        row.sort_by_cached_key(|h| h.catalog_name.to_lowercase());
    }
    rows
}

/// The volumes of a row, as shown in its *Volume* column: `Backup 1, Backup 2`.
#[must_use]
pub fn volume_names(row: &[Hit]) -> String {
    row.iter().map(|h| h.catalog_name.as_str()).collect::<Vec<_>>().join(", ")
}

/// Sorts search rows by a column of their first hit (so by their first volume for the *Volume* column); ties are
/// broken by name.
pub fn sort_rows(rows: &mut [HitRow], key: SortKey, ascending: bool) {
    rows.sort_by(|a, b| {
        let (a, b) = (&a[0], &b[0]);
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

/// Entry ids of the hits, by catalog file: the restriction of a "search within results" (ids are per catalog file).
#[must_use]
pub fn ids_by_catalog<'a>(hits: impl IntoIterator<Item = &'a Hit>) -> HashMap<PathBuf, Vec<i64>> {
    let mut ids: HashMap<PathBuf, Vec<i64>> = HashMap::new();
    for hit in hits {
        ids.entry(hit.catalog_file.clone()).or_default().push(hit.entry.id);
    }
    ids
}

/// Number of selected rows and their total size (folders count as 0).
#[must_use]
pub fn selection_total(sizes: impl Iterator<Item = Option<u64>>) -> (usize, u64) {
    sizes.fold((0, 0), |(n, total), size| (n + 1, total + size.unwrap_or(0)))
}

/// Tab-separated rows, one per line, ready to paste into a spreadsheet. Tabs and line breaks inside cells become
/// spaces.
#[must_use]
pub fn rows_to_tsv(rows: &[Vec<String>]) -> String {
    let clean = |cell: &str| cell.replace(['\t', '\r', '\n'], " ");
    rows.iter().map(|row| row.iter().map(|c| clean(c)).collect::<Vec<_>>().join("\t")).collect::<Vec<_>>().join(NEWLINE)
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

/// Sub-folders of every folder (`None` = top of the source), each list sorted by name: (id, name).
pub type DirTree = HashMap<Option<i64>, Vec<(i64, String)>>;

/// One visible line of a folder tree.
#[derive(Debug, PartialEq, Eq)]
pub struct TreeRow<'a> {
    /// For each folder from the top level down to this one (empty for the root): whether a sibling follows it below,
    /// i.e. whether the tree's connector line continues past this row at that level. Its length is the nesting level.
    pub follows: Vec<bool>,
    /// Folder id, `None` for the root.
    pub id: Option<i64>,
    /// Displayed name.
    pub name: &'a str,
    /// Whether the folder has sub-folders (and so can be expanded).
    pub has_children: bool,
}

/// Visible lines of a folder tree, depth first: the root (named `root`), then the sub-folders of every expanded one.
#[must_use]
pub fn visible_tree<'a>(root: &'a str, tree: &'a DirTree, expanded: &HashSet<Option<i64>>) -> Vec<TreeRow<'a>> {
    let has_children = |id: Option<i64>| tree.get(&id).is_some_and(|k| !k.is_empty());
    let mut out = Vec::new();
    // Explicit stack (children pushed in reverse) keeps the name order without recursion depth limits.
    let mut stack = vec![(Vec::new(), None, root)];
    while let Some((follows, id, name)) = stack.pop() {
        if expanded.contains(&id) {
            let kids = tree.get(&id).map_or(&[][..], Vec::as_slice);
            stack.extend(kids.iter().enumerate().rev().map(|(i, (c, n))| {
                let mut chain = follows.clone();
                chain.push(i + 1 < kids.len());
                (chain, Some(*c), n.as_str())
            }));
        }
        out.push(TreeRow { follows, id, name, has_children: has_children(id) });
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

    use std::collections::HashSet;

    use super::{
        DirTree, HitRow, NEWLINE, SortKey, TreeRow, filter_catalogs, format_timestamp, hit_rows, ids_by_catalog,
        rows_to_tsv, selection_total, sort_entries, sort_rows, tree_text, visible_tree, volume_names,
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

    #[test]
    fn ids_grouped_by_catalog() {
        let mut hits = hits();
        hits[0].entry.id = 7;
        hits.push(hit("Z", false, "gamma.txt", Some(1)));
        let ids = ids_by_catalog(&hits);
        assert_eq!(ids.len(), 3);
        // The same id in two catalogs stays apart.
        assert_eq!(ids[&std::path::PathBuf::from("Z.db")], [7, 1]);
        assert_eq!(ids[&std::path::PathBuf::from("A.db")], [1]);
        assert!(ids_by_catalog(&[] as &[Hit]).is_empty());
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

    fn order(rows: &[HitRow]) -> Vec<&str> {
        rows.iter().map(|r| r[0].entry.name.as_str()).collect()
    }

    #[test]
    fn sort_rows_each_key() {
        let mut h = hit_rows(hits(), false);
        sort_rows(&mut h, SortKey::Name, true);
        assert_eq!(order(&h), ["Alpha.txt", "beta.txt", "dir"]);
        sort_rows(&mut h, SortKey::Name, false);
        assert_eq!(order(&h), ["dir", "beta.txt", "Alpha.txt"]);
        sort_rows(&mut h, SortKey::Size, true);
        assert_eq!(order(&h), ["dir", "beta.txt", "Alpha.txt"]);
        sort_rows(&mut h, SortKey::Date, true);
        assert_eq!(order(&h), ["Alpha.txt", "dir", "beta.txt"]);
        sort_rows(&mut h, SortKey::Path, true);
        assert_eq!(order(&h), ["dir", "beta.txt", "Alpha.txt"]);
        sort_rows(&mut h, SortKey::Catalog, false);
        assert_eq!(order(&h), ["beta.txt", "dir", "Alpha.txt"]);
    }

    #[test]
    fn identical_hits_grouped() {
        let at = |catalog: &str, name: &str, size: Option<u64>, mtime: i64, path: &str| {
            let mut h = hit(catalog, false, name, size);
            h.entry.mtime = Some(mtime);
            h.path = path.to_owned();
            h
        };
        let hits = vec![
            at("NAS", "a.txt", Some(1), 10, "/x"),
            at("Backup 2", "a.txt", Some(1), 10, "/x"),
            at("Backup 1", "a.txt", Some(1), 10, "/x"),
            // Each differs from the a.txt above in one field only.
            at("Backup 1", "A.txt", Some(1), 10, "/x"),
            at("Backup 1", "a.txt", Some(2), 10, "/x"),
            at("Backup 1", "a.txt", Some(1), 11, "/x"),
            at("Backup 1", "a.txt", Some(1), 10, "/y"),
            // Folders have no size: name, date and path decide.
            at("NAS", "dir", None, 5, "/"),
            at("Backup 1", "dir", None, 5, "/"),
        ];
        let rows = hit_rows(hits.clone(), true);
        let shown: Vec<(&str, String)> = rows.iter().map(|r| (r[0].entry.name.as_str(), volume_names(r))).collect();
        assert_eq!(
            shown,
            [
                ("a.txt", "Backup 1, Backup 2, NAS".to_owned()),
                ("A.txt", "Backup 1".to_owned()),
                ("a.txt", "Backup 1".to_owned()),
                ("a.txt", "Backup 1".to_owned()),
                ("a.txt", "Backup 1".to_owned()),
                ("dir", "Backup 1, NAS".to_owned()),
            ]
        );
        // Not grouped: one row per hit, unchanged.
        let single = hit_rows(hits.clone(), false);
        assert_eq!(single.into_iter().flatten().collect::<Vec<_>>(), hits);
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
        assert_eq!(rows_to_tsv(&rows), format!("a b\t1{NEWLINE}line break\t2"));
    }

    #[test]
    fn tree_indentation() {
        let dirs = vec![(0, "A".to_owned()), (1, "B".to_owned()), (0, "C".to_owned())];
        assert_eq!(tree_text("B1:\\BD", &dirs), format!("B1:\\BD{NEWLINE}  A{NEWLINE}    B{NEWLINE}  C{NEWLINE}"));
    }

    #[test]
    fn tree_rows_follow_expansion() {
        let tree: DirTree = [
            (None, vec![(1, "A".to_owned()), (2, "Z".to_owned())]),
            (Some(1), vec![(3, "B".to_owned())]),
            (Some(3), vec![(4, "C".to_owned())]),
        ]
        .into_iter()
        .collect();
        let row =
            |follows: &[bool], id, name, has_children| TreeRow { follows: follows.to_vec(), id, name, has_children };
        assert_eq!(visible_tree("B1", &tree, &HashSet::new()), [row(&[], None, "B1", true)]);
        // A collapsed folder hides its sub-folders even when they are expanded themselves.
        let expanded = HashSet::from([None, Some(3)]);
        assert_eq!(
            visible_tree("B1", &tree, &expanded),
            [row(&[], None, "B1", true), row(&[true], Some(1), "A", true), row(&[false], Some(2), "Z", false)]
        );
        // The line of A continues down to Z, past B and C; B and C are the last of their siblings.
        let expanded = HashSet::from([None, Some(1), Some(3)]);
        let rows: Vec<(Vec<bool>, &str)> =
            visible_tree("B1", &tree, &expanded).into_iter().map(|r| (r.follows, r.name)).collect();
        let expected: Vec<(Vec<bool>, &str)> = vec![
            (vec![], "B1"),
            (vec![true], "A"),
            (vec![true, false], "B"),
            (vec![true, false, false], "C"),
            (vec![false], "Z"),
        ];
        assert_eq!(rows, expected);
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
