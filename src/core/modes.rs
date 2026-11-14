//! Cathy's search modes, applied to the hits of the matching entries.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use super::model::Hit;

/// A search mode (§5.2 of the design).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    /// Every matching entry.
    #[default]
    Standard,
    /// Files sharing name and size with another file, folders sharing name and children names with another folder.
    Duplicates,
    /// Files sharing their name with another file.
    SameNames,
    /// Files sharing their size with another file.
    SameSizes,
    /// Files with no other file of the same name and size.
    Singles,
    /// Entries whose parent folder name matches (handled by the query; identity here).
    Parents,
    /// Files of non-archive catalogs missing from every archive catalog.
    NotArchived,
    /// Files of non-archive catalogs present in some archive catalog.
    Archived,
}

impl SearchMode {
    /// Every mode, in the order shown in the interface.
    pub const ALL: [SearchMode; 8] = [
        SearchMode::Standard,
        SearchMode::Duplicates,
        SearchMode::SameNames,
        SearchMode::SameSizes,
        SearchMode::Singles,
        SearchMode::Parents,
        SearchMode::NotArchived,
        SearchMode::Archived,
    ];

    /// Whether the mode compares entries with each other (and so needs every match, not just the first ones).
    #[must_use]
    pub fn is_grouping(self) -> bool {
        !matches!(self, SearchMode::Standard | SearchMode::Parents)
    }

    /// Whether the mode exists to show the same file on several volumes, so identical results must not be grouped.
    #[must_use]
    pub fn shows_copies(self) -> bool {
        matches!(self, SearchMode::Duplicates | SearchMode::SameNames | SearchMode::SameSizes)
    }
}

/// The identity of a file for duplicate detection: folded name and size.
#[must_use]
pub fn key(hit: &Hit) -> (String, u64) {
    (hit.name_fold.clone(), hit.entry.size.unwrap_or(0))
}

/// Fingerprint of a folder's direct children names, whatever their order: two folders with the same names inside
/// get the same value. Only compared within one run.
#[must_use]
pub fn children_fingerprint<'a>(names: impl IntoIterator<Item = &'a str>) -> u64 {
    let mut names: Vec<&str> = names.into_iter().collect();
    names.sort_unstable();
    let mut hasher = DefaultHasher::new();
    names.hash(&mut hasher);
    hasher.finish()
}

/// The identity of an entry for the Duplicates mode: folded name, then size for a file, children for a folder.
fn duplicate_key(hit: &Hit) -> (String, Option<u64>, Option<u64>) {
    (hit.name_fold.clone(), hit.entry.size, hit.children)
}

/// Keeps the hits the mode selects. Grouping modes other than Duplicates consider files only and return each group's members next to
/// each other, groups sorted by key. `archive_keys` are the keys of the files of the archive catalogs.
#[must_use]
pub fn apply_mode(mode: SearchMode, hits: Vec<Hit>, archive_keys: &HashSet<(String, u64)>) -> Vec<Hit> {
    let files = || hits.iter().filter(|h| !h.entry.is_dir).cloned().collect::<Vec<_>>();
    match mode {
        SearchMode::Standard | SearchMode::Parents => hits,
        SearchMode::Duplicates => grouped(hits, duplicate_key, |n| n >= 2),
        SearchMode::SameNames => grouped(files(), |h| h.name_fold.clone(), |n| n >= 2),
        SearchMode::SameSizes => grouped(files(), |h| h.entry.size.unwrap_or(0), |n| n >= 2),
        SearchMode::Singles => grouped(files(), key, |n| n == 1),
        SearchMode::NotArchived | SearchMode::Archived => {
            let wanted = mode == SearchMode::Archived;
            files().into_iter().filter(|h| !h.catalog_archive && archive_keys.contains(&key(h)) == wanted).collect()
        }
    }
}

/// Keeps the hits whose key occurs a number of times accepted by `keep`, stably sorted by key.
fn grouped<K: Ord + std::hash::Hash + Clone>(
    hits: Vec<Hit>,
    key: impl Fn(&Hit) -> K,
    keep: impl Fn(usize) -> bool,
) -> Vec<Hit> {
    let mut counts: HashMap<K, usize> = HashMap::new();
    for hit in &hits {
        *counts.entry(key(hit)).or_default() += 1;
    }
    let mut kept: Vec<(K, Hit)> = hits.into_iter().map(|h| (key(&h), h)).filter(|(k, _)| keep(counts[k])).collect();
    kept.sort_by(|a, b| a.0.cmp(&b.0));
    kept.into_iter().map(|(_, h)| h).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashSet;
    use std::path::PathBuf;

    use super::{SearchMode, apply_mode, children_fingerprint};
    use crate::core::model::{Entry, Hit};

    pub(crate) fn hit(catalog: &str, archive: bool, name: &str, size: Option<u64>) -> Hit {
        Hit {
            catalog_name: catalog.to_owned(),
            catalog_file: PathBuf::from(format!("{catalog}.db")),
            catalog_archive: archive,
            entry: Entry { id: 1, parent_id: None, name: name.to_owned(), is_dir: size.is_none(), size, mtime: None },
            name_fold: name.to_lowercase(),
            path: String::new(),
            children: size.is_none().then(|| children_fingerprint([])),
        }
    }

    fn sample() -> Vec<Hit> {
        vec![
            hit("A", false, "b.txt", Some(5)),
            hit("A", false, "a.txt", Some(1)),
            hit("B", false, "A.TXT", Some(1)),
            hit("B", false, "a.txt", Some(2)),
            hit("B", false, "c.txt", Some(5)),
            hit("B", false, "a.txt", None),
        ]
    }

    fn view(hits: &[Hit]) -> Vec<(&str, &str, Option<u64>)> {
        hits.iter().map(|h| (h.catalog_name.as_str(), h.entry.name.as_str(), h.entry.size)).collect()
    }

    #[test]
    fn copy_modes() {
        let copies: Vec<SearchMode> = SearchMode::ALL.into_iter().filter(|m| m.shows_copies()).collect();
        assert_eq!(copies, [SearchMode::Duplicates, SearchMode::SameNames, SearchMode::SameSizes]);
    }

    #[test]
    fn standard_and_parents_keep_everything() {
        for mode in [SearchMode::Standard, SearchMode::Parents] {
            assert_eq!(apply_mode(mode, sample(), &HashSet::new()), sample());
        }
    }

    #[test]
    fn duplicates_need_name_and_size() {
        let out = apply_mode(SearchMode::Duplicates, sample(), &HashSet::new());
        assert_eq!(view(&out), [("A", "a.txt", Some(1)), ("B", "A.TXT", Some(1))]);
    }

    #[test]
    fn duplicate_folders_need_name_and_children() {
        let dir = |catalog: &str, name: &str, children: &[&str]| {
            let mut h = hit(catalog, false, name, None);
            h.children = Some(children_fingerprint(children.iter().copied()));
            h
        };
        let hits = vec![
            dir("A", "Photos", &["a.jpg", "b.jpg"]),
            dir("B", "photos", &["b.jpg", "a.jpg"]),
            dir("C", "Photos", &["a.jpg"]),
            dir("A", "Empty", &[]),
            dir("B", "Empty", &[]),
            hit("C", false, "Empty", Some(0)),
        ];
        let out = apply_mode(SearchMode::Duplicates, hits, &HashSet::new());
        assert_eq!(
            view(&out),
            [("A", "Empty", None), ("B", "Empty", None), ("A", "Photos", None), ("B", "photos", None)]
        );
    }

    #[test]
    fn fingerprint_ignores_order_only() {
        assert_eq!(children_fingerprint(["a", "b"]), children_fingerprint(["b", "a"]));
        assert_ne!(children_fingerprint(["a", "b"]), children_fingerprint(["a"]));
        assert_ne!(children_fingerprint(["ab"]), children_fingerprint(["a", "b"]));
        assert_ne!(children_fingerprint(["A"]), children_fingerprint(["a"]));
    }

    #[test]
    fn same_names_grouped() {
        let out = apply_mode(SearchMode::SameNames, sample(), &HashSet::new());
        assert_eq!(view(&out), [("A", "a.txt", Some(1)), ("B", "A.TXT", Some(1)), ("B", "a.txt", Some(2))]);
    }

    #[test]
    fn same_sizes_grouped() {
        let out = apply_mode(SearchMode::SameSizes, sample(), &HashSet::new());
        assert_eq!(
            view(&out),
            [("A", "a.txt", Some(1)), ("B", "A.TXT", Some(1)), ("A", "b.txt", Some(5)), ("B", "c.txt", Some(5))]
        );
    }

    #[test]
    fn singles_are_unique_files() {
        let out = apply_mode(SearchMode::Singles, sample(), &HashSet::new());
        assert_eq!(view(&out), [("B", "a.txt", Some(2)), ("A", "b.txt", Some(5)), ("B", "c.txt", Some(5))]);
    }

    #[test]
    fn archived_and_not_archived() {
        let hits = vec![
            hit("Arch", true, "x.txt", Some(1)),
            hit("Work", false, "x.txt", Some(1)),
            hit("Work", false, "y.txt", Some(1)),
            hit("Work", false, "dir", None),
        ];
        let keys: HashSet<(String, u64)> = [("x.txt".to_owned(), 1)].into();
        assert_eq!(view(&apply_mode(SearchMode::NotArchived, hits.clone(), &keys)), [("Work", "y.txt", Some(1))]);
        assert_eq!(view(&apply_mode(SearchMode::Archived, hits, &keys)), [("Work", "x.txt", Some(1))]);
    }
}
