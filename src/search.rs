//! Searching several catalogs at once, in parallel, with Cathy's modes.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::catalog::query::{child_names, dir_paths, parent_path, search_catalog};
use crate::catalog::store::{CatalogInfo, open_read};
use crate::core::model::{Hit, KindFilter, SearchQuery};
use crate::core::modes::{SearchMode, apply_mode, children_fingerprint, key};

/// Outcome of a search over several catalogs.
#[derive(Debug, Default)]
pub struct SearchResult {
    /// Matching entries, at most the requested limit, with their folder paths and children fingerprints filled.
    pub hits: Vec<Hit>,
    /// Whether more entries matched than the limit allowed.
    pub truncated: bool,
    /// Catalogs that could not be searched: (catalog name, error).
    pub errors: Vec<(String, String)>,
    /// Time spent.
    pub elapsed: Duration,
}

/// Searches `catalogs` and returns at most `limit` hits.
///
/// With `within` (entry ids by catalog file, see [`crate::core::listing::ids_by_catalog`]), only the catalogs it
/// lists are searched, each restricted to its own ids: a search within previous results. The archive reference side of
/// the archive modes is never restricted: it only tells whether an archived copy exists.
///
/// Each catalog gets its own connection on a shared pool of `max(1, cores - 1)` threads, so the machine stays
/// responsive. A failing catalog is reported in [`SearchResult::errors`] without stopping the others.
#[must_use]
pub fn search(
    catalogs: &[CatalogInfo],
    q: &SearchQuery,
    mode: SearchMode,
    limit: usize,
    within: Option<&HashMap<PathBuf, Vec<i64>>>,
) -> SearchResult {
    let started = Instant::now();
    let archive_mode = matches!(mode, SearchMode::NotArchived | SearchMode::Archived);
    // Grouping modes compare entries with each other: they need every match. Only Duplicates compares folders.
    let files_only = mode.is_grouping() && mode != SearchMode::Duplicates;
    let query = if files_only { SearchQuery { kind: KindFilter::Files, ..q.clone() } } else { q.clone() };
    let per_catalog = (!mode.is_grouping()).then_some(limit.saturating_add(1));

    let candidates: Vec<&CatalogInfo> = catalogs
        .iter()
        .filter(|c| (!archive_mode || !c.volume.archive) && within.is_none_or(|w| w.contains_key(&c.file)))
        .collect();
    let (hits, mut errors) = run(&candidates, &query, per_catalog, within);
    let mut archive_keys = HashSet::new();
    if archive_mode {
        // The archive side only needs the same names; other filters must not hide an archived copy.
        let references: Vec<&CatalogInfo> = catalogs.iter().filter(|c| c.volume.archive).collect();
        let by_name = SearchQuery { pattern: q.pattern.clone(), kind: KindFilter::Files, ..SearchQuery::all() };
        let (reference_hits, reference_errors) = run(&references, &by_name, None, None);
        archive_keys = reference_hits.iter().map(key).collect();
        errors.extend(reference_errors);
    }

    let mut hits = hits;
    fill_children(&mut hits, &mut errors);
    let mut hits = apply_mode(mode, hits, &archive_keys);
    let truncated = hits.len() > limit;
    hits.truncate(limit);
    fill_paths(&mut hits, &mut errors);
    SearchResult { hits, truncated, errors, elapsed: started.elapsed() }
}

/// The search thread pool.
fn pool() -> &'static rayon::ThreadPool {
    static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();
    POOL.get_or_init(|| {
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get().saturating_sub(1).max(1));
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("oxycat-search-{i}"))
            .build()
            .expect("cannot start the search thread pool")
    })
}

/// Runs `q` on each catalog in parallel, restricted to its ids in `within` if given; returns all hits and the
/// per-catalog errors.
fn run(
    catalogs: &[&CatalogInfo],
    q: &SearchQuery,
    limit: Option<usize>,
    within: Option<&HashMap<PathBuf, Vec<i64>>>,
) -> (Vec<Hit>, Vec<(String, String)>) {
    let results: Vec<(String, anyhow::Result<Vec<Hit>>)> = pool().install(|| {
        catalogs
            .par_iter()
            .map(|info| {
                let ids = within.and_then(|w| w.get(&info.file)).map(Vec::as_slice);
                let found = open_read(&info.file).and_then(|conn| Ok(search_catalog(&conn, q, info, limit, ids)?));
                (info.volume.name.clone(), found)
            })
            .collect()
    });
    let mut hits = Vec::new();
    let mut errors = Vec::new();
    for (name, found) in results {
        match found {
            Ok(found) => hits.extend(found),
            Err(e) => errors.push((name, format!("{e:#}"))),
        }
    }
    (hits, errors)
}

/// Fills [`Hit::children`] for the folders of `hits`, opening each catalog holding some once.
fn fill_children(hits: &mut [Hit], errors: &mut Vec<(String, String)>) {
    let mut dirs: HashMap<PathBuf, (String, Vec<i64>)> = HashMap::new();
    for hit in hits.iter().filter(|h| h.entry.is_dir) {
        dirs.entry(hit.catalog_file.clone())
            .or_insert_with(|| (hit.catalog_name.clone(), Vec::new()))
            .1
            .push(hit.entry.id);
    }
    let mut names: HashMap<PathBuf, HashMap<i64, Vec<String>>> = HashMap::new();
    for (file, (catalog, ids)) in dirs {
        match open_read(&file).and_then(|conn| Ok(child_names(&conn, &ids)?)) {
            Ok(found) => {
                names.insert(file, found);
            }
            Err(e) => errors.push((catalog, format!("{e:#}"))),
        }
    }
    for hit in hits.iter_mut().filter(|h| h.entry.is_dir) {
        // Unknown children (unreadable catalog) never match a real fingerprint.
        hit.children = names
            .get(&hit.catalog_file)
            .map(|by_dir| children_fingerprint(by_dir.get(&hit.entry.id).into_iter().flatten().map(String::as_str)));
    }
}

/// Fills [`Hit::path`], opening each catalog present in `hits` once.
fn fill_paths(hits: &mut [Hit], errors: &mut Vec<(String, String)>) {
    let mut paths: HashMap<PathBuf, HashMap<i64, String>> = HashMap::new();
    for hit in hits.iter_mut() {
        if !paths.contains_key(&hit.catalog_file) {
            let loaded = open_read(&hit.catalog_file).and_then(|conn| Ok(dir_paths(&conn)?));
            match loaded {
                Ok(map) => paths.insert(hit.catalog_file.clone(), map),
                Err(e) => {
                    errors.push((hit.catalog_name.clone(), format!("{e:#}")));
                    paths.insert(hit.catalog_file.clone(), HashMap::new())
                }
            };
        }
        hit.path = parent_path(&paths[&hit.catalog_file], hit.entry.parent_id);
    }
}

#[cfg(test)]
mod tests {
    use std::path::MAIN_SEPARATOR as SEP;

    use super::search;
    use crate::catalog::query::tests::catalog_of;
    use crate::catalog::store::CatalogInfo;
    use crate::core::listing::ids_by_catalog;
    use crate::core::model::{KindFilter, SearchQuery};
    use crate::core::modes::SearchMode;
    use crate::core::pattern::Expr;

    fn q(pattern: &str) -> SearchQuery {
        SearchQuery { pattern: Expr::parse(pattern).unwrap(), ..SearchQuery::all() }
    }

    fn two() -> (Vec<tempfile::TempDir>, Vec<CatalogInfo>) {
        let (t1, a) = catalog_of("A", &[("BD/Astérix 01.cbz", Some(3)), ("x.txt", Some(1))]);
        let (t2, b) = catalog_of("B", &[("asterix 02.cbz", Some(4)), ("x.txt", Some(1)), ("y.txt", Some(1))]);
        (vec![t1, t2], vec![a, b])
    }

    #[test]
    fn merges_catalogs() {
        let (_t, cats) = two();
        let result = search(&cats, &q("asterix"), SearchMode::Standard, 100, None);
        let mut found: Vec<(String, String)> =
            result.hits.iter().map(|h| (h.catalog_name.clone(), h.entry.name.clone())).collect();
        found.sort();
        assert_eq!(
            found,
            [("A".to_owned(), "Astérix 01.cbz".to_owned()), ("B".to_owned(), "asterix 02.cbz".to_owned())]
        );
        assert!(!result.truncated);
        assert_eq!(result.errors, [] as [(String, String); 0]);
    }

    #[test]
    fn truncates_and_flags() {
        let (_t, cats) = two();
        let result = search(&cats, &q(""), SearchMode::Standard, 2, None);
        assert_eq!(result.hits.len(), 2);
        assert!(result.truncated);
        let exact = search(&cats, &q("asterix"), SearchMode::Standard, 2, None);
        assert!(!exact.truncated);
    }

    #[test]
    fn paths_filled() {
        let (_t, cats) = two();
        let result = search(&cats, &q("astérix 01"), SearchMode::Standard, 10, None);
        assert_eq!(result.hits[0].path, format!("{SEP}BD"));
        let top = search(&cats, &q("y.txt"), SearchMode::Standard, 10, None);
        assert_eq!(top.hits[0].path, SEP.to_string());
    }

    #[test]
    fn grouping_mode_spans_catalogs() {
        let (_t, cats) = two();
        let result = search(&cats, &q(".txt|"), SearchMode::Duplicates, 10, None);
        let found: Vec<String> = result.hits.iter().map(|h| format!("{}/{}", h.catalog_name, h.entry.name)).collect();
        assert_eq!(found.len(), 2);
        assert!(found.contains(&"A/x.txt".to_owned()) && found.contains(&"B/x.txt".to_owned()), "{found:?}");
    }

    #[test]
    fn duplicate_folders_compare_children() {
        let (t1, a) = catalog_of("A", &[("Photos/a.jpg", Some(1)), ("Photos/b.jpg", Some(2)), ("Docs/x.txt", Some(1))]);
        let (t2, b) = catalog_of("B", &[("Photos/b.jpg", Some(5)), ("Photos/a.jpg", Some(5)), ("Docs/y.txt", Some(1))]);
        let cats = vec![a, b];
        let result = search(&cats, &SearchQuery { kind: KindFilter::Dirs, ..q("") }, SearchMode::Duplicates, 10, None);
        let found: Vec<String> = result.hits.iter().map(|h| format!("{}/{}", h.catalog_name, h.entry.name)).collect();
        assert_eq!(found, ["A/Photos", "B/Photos"]);
        // Every folder gets its fingerprint, whatever the mode.
        let all = search(&cats, &q(""), SearchMode::Standard, 10, None);
        assert!(all.hits.iter().all(|h| h.children.is_some() == h.entry.is_dir), "{:?}", all.hits);
        drop((t1, t2));
    }

    #[test]
    fn archive_modes_use_archive_catalogs() {
        let (_t, mut cats) = two();
        cats[0].volume.archive = true;
        let not = search(&cats, &q(".txt|"), SearchMode::NotArchived, 10, None);
        assert_eq!(not.hits.iter().map(|h| h.entry.name.as_str()).collect::<Vec<_>>(), ["y.txt"]);
        let arch = search(&cats, &q(".txt|"), SearchMode::Archived, 10, None);
        assert_eq!(arch.hits.iter().map(|h| h.entry.name.as_str()).collect::<Vec<_>>(), ["x.txt"]);
        assert_eq!(arch.hits[0].catalog_name, "B");
        // The archive side uses the same expression.
        let not = search(&cats, &q("x.txt OR y.txt"), SearchMode::NotArchived, 10, None);
        assert_eq!(not.hits.iter().map(|h| h.entry.name.as_str()).collect::<Vec<_>>(), ["y.txt"]);
    }

    #[test]
    fn broken_catalog_reported_others_still_searched() {
        let (t, mut cats) = two();
        let broken = t[0].path().join("broken.db");
        std::fs::write(&broken, b"not sqlite at all, just text").unwrap();
        let mut bad = cats[0].clone();
        bad.file = broken;
        bad.volume.name = "Broken".to_owned();
        cats.push(bad);
        let result = search(&cats, &q("x.txt"), SearchMode::Standard, 10, None);
        assert_eq!(result.hits.len(), 2);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].0, "Broken");
    }

    #[test]
    fn within_previous_results() {
        let (_t, cats) = two();
        let first = search(&cats, &q(".cbz| OR x.txt"), SearchMode::Standard, 10, None);
        assert_eq!(first.hits.len(), 4);
        // Only entries of the first results matching the new query; repeatable.
        let ids = ids_by_catalog(&first.hits);
        let second = search(&cats, &q("asterix OR y.txt"), SearchMode::Standard, 10, Some(&ids));
        let mut found: Vec<String> =
            second.hits.iter().map(|h| format!("{}/{}", h.catalog_name, h.entry.name)).collect();
        found.sort();
        assert_eq!(found, ["A/Astérix 01.cbz", "B/asterix 02.cbz"]);
        let ids = ids_by_catalog(&second.hits);
        let third = search(&cats, &q("02"), SearchMode::Standard, 10, Some(&ids));
        assert_eq!(third.hits.iter().map(|h| h.entry.name.as_str()).collect::<Vec<_>>(), ["asterix 02.cbz"]);
        assert_eq!(third.hits[0].path, SEP.to_string());
    }

    #[test]
    fn within_keeps_ids_per_catalog() {
        let (_t, cats) = two();
        // A's ids only: B shares them but is not searched at all.
        let from_a = search(&cats[..1], &q(""), SearchMode::Standard, 10, None);
        let ids = ids_by_catalog(&from_a.hits);
        let refined = search(&cats, &q(""), SearchMode::Standard, 10, Some(&ids));
        assert!(refined.hits.iter().all(|h| h.catalog_name == "A"), "{:?}", refined.hits);
        assert_eq!(refined.hits.len(), from_a.hits.len());
    }

    #[test]
    fn within_archive_reference_unrestricted() {
        let (_t, mut cats) = two();
        cats[0].volume.archive = true;
        let txt = search(&cats, &q(".txt|"), SearchMode::Standard, 10, None);
        // Keep only B's entries: A (the archive) is absent from the restriction yet still the reference.
        let only_b: Vec<_> = txt.hits.into_iter().filter(|h| h.catalog_name == "B").collect();
        let ids = ids_by_catalog(&only_b);
        let not = search(&cats, &q(""), SearchMode::NotArchived, 10, Some(&ids));
        assert_eq!(not.hits.iter().map(|h| h.entry.name.as_str()).collect::<Vec<_>>(), ["y.txt"]);
    }
}
