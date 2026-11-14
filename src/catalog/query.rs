//! Read queries on one catalog: folder contents, ancestors, folder paths, search.

use std::collections::HashMap;
use std::path::MAIN_SEPARATOR;

use rusqlite::{Connection, ToSql, params_from_iter};

use super::store::CatalogInfo;
use crate::core::listing::DirTree;
use crate::core::model::{Entry, Hit, KindFilter, SearchQuery};

/// Entries of a folder (`None` = top of the cataloged source): folders first, then by folded name.
///
/// # Errors
/// Any SQLite error.
pub fn children(conn: &Connection, parent: Option<i64>) -> rusqlite::Result<Vec<Entry>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {ENTRY_COLUMNS} FROM entries WHERE parent_id IS ?1 ORDER BY is_dir DESC, name_fold, name"
    ))?;
    stmt.query_map([parent], entry_from_row)?.collect()
}

/// The chain of folders from the top of the source down to `id` (inclusive).
///
/// # Errors
/// Any SQLite error.
pub fn ancestors(conn: &Connection, id: i64) -> rusqlite::Result<Vec<Entry>> {
    let mut stmt = conn.prepare_cached(&format!(
        "WITH RECURSIVE chain(id, depth) AS (SELECT ?1, 0 UNION ALL
           SELECT e.parent_id, chain.depth + 1 FROM entries e JOIN chain ON e.id = chain.id WHERE e.parent_id IS NOT NULL)
         SELECT {ENTRY_COLUMNS} FROM chain JOIN entries USING (id) ORDER BY depth DESC"
    ))?;
    stmt.query_map([id], entry_from_row)?.collect()
}

/// Path of every folder from the top of the source, with a leading separator (`\BD\Astérix` on Windows).
///
/// # Errors
/// Any SQLite error.
pub fn dir_paths(conn: &Connection) -> rusqlite::Result<HashMap<i64, String>> {
    let mut stmt = conn.prepare("SELECT id, parent_id, name FROM entries WHERE is_dir = 1 ORDER BY id")?;
    let mut paths = HashMap::new();
    // A folder is always inserted after its parent, so its parent's path is already known.
    for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, String>(2)?)))? {
        let (id, parent, name) = row?;
        let base = parent.and_then(|p| paths.get(&p).cloned()).unwrap_or_default();
        paths.insert(id, format!("{base}{MAIN_SEPARATOR}{name}"));
    }
    Ok(paths)
}

/// Path of a parent folder from [`dir_paths`]; a lone separator for the top of the source.
#[must_use]
pub fn parent_path<S: std::hash::BuildHasher>(paths: &HashMap<i64, String, S>, parent: Option<i64>) -> String {
    parent.and_then(|p| paths.get(&p).cloned()).unwrap_or_else(|| MAIN_SEPARATOR.to_string())
}

/// Columns read by [`entry_from_row`].
const ENTRY_COLUMNS: &str = "id, parent_id, name, is_dir, size, mtime";

/// Builds an [`Entry`] from a row selecting [`ENTRY_COLUMNS`] first.
fn entry_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: r.get(0)?,
        parent_id: r.get(1)?,
        name: r.get(2)?,
        is_dir: r.get(3)?,
        size: r.get(4)?,
        mtime: r.get(5)?,
    })
}

/// Entries of one catalog matching a query, at most `limit` of them. Paths are left empty.
///
/// # Errors
/// Any SQLite error.
pub fn search_catalog(
    conn: &Connection,
    q: &SearchQuery,
    info: &CatalogInfo,
    limit: Option<usize>,
) -> rusqlite::Result<Vec<Hit>> {
    let mut likes = Vec::new();
    let column = if q.parents { "p.name_fold" } else { "e.name_fold" };
    let pattern = (!q.pattern.is_empty()).then(|| q.pattern.to_sql(column, &mut likes));
    let mut conditions: Vec<&str> = pattern.as_deref().into_iter().collect();
    let mut values: Vec<Box<dyn ToSql>> = likes.into_iter().map(|like| Box::new(like) as Box<dyn ToSql>).collect();
    let mut bound = |condition: &'static str, value: Option<Box<dyn ToSql>>| {
        if let Some(value) = value {
            conditions.push(condition);
            values.push(value);
        }
    };
    bound("e.size >= ?", q.min_size.map(|v| Box::new(v) as Box<dyn ToSql>));
    bound("e.size <= ?", q.max_size.map(|v| Box::new(v) as Box<dyn ToSql>));
    bound("e.mtime >= ?", q.from.map(|v| Box::new(v) as Box<dyn ToSql>));
    bound("e.mtime < ?", q.to.map(|v| Box::new(v) as Box<dyn ToSql>));
    match q.kind {
        KindFilter::All => {}
        KindFilter::Files => conditions.push("e.is_dir = 0"),
        KindFilter::Dirs => conditions.push("e.is_dir = 1"),
    }
    let join = if q.parents { "JOIN entries p ON p.id = e.parent_id" } else { "" };
    let filter = if conditions.is_empty() { String::new() } else { format!("WHERE {}", conditions.join(" AND ")) };
    let limit = limit.map(|n| format!("LIMIT {n}")).unwrap_or_default();
    let sql = format!(
        "SELECT e.id, e.parent_id, e.name, e.is_dir, e.size, e.mtime, e.name_fold FROM entries e {join} {filter} {limit}"
    );
    let mut stmt = conn.prepare(&sql)?;
    stmt.query_map(params_from_iter(values.iter()), |r| {
        Ok(Hit {
            catalog_name: info.volume.name.clone(),
            catalog_file: info.file.clone(),
            catalog_archive: info.volume.archive,
            entry: entry_from_row(r)?,
            name_fold: r.get(6)?,
            path: String::new(),
        })
    })?
    .collect()
}

/// Sub-folders of every folder of the catalog, by folded name.
///
/// # Errors
/// Any SQLite error.
pub fn dir_tree(conn: &Connection) -> rusqlite::Result<DirTree> {
    let mut stmt = conn.prepare("SELECT id, parent_id, name FROM entries WHERE is_dir = 1 ORDER BY name_fold, name")?;
    let mut kids = DirTree::new();
    for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, String>(2)?)))? {
        let (id, parent, name) = row?;
        kids.entry(parent).or_default().push((id, name));
    }
    Ok(kids)
}

/// Every folder below `dir` (`None` = whole catalog) with its depth (0 = direct child), depth first, by name.
///
/// # Errors
/// Any SQLite error.
pub fn subtree_dirs(conn: &Connection, dir: Option<i64>) -> rusqlite::Result<Vec<(usize, String)>> {
    let kids = dir_tree(conn)?;
    let mut out = Vec::new();
    // Explicit stack (children pushed in reverse) keeps the name order without recursion depth limits.
    let mut stack: Vec<(usize, i64, String)> =
        kids.get(&dir).into_iter().flatten().rev().map(|(id, name)| (0, *id, name.clone())).collect();
    while let Some((depth, id, name)) = stack.pop() {
        stack.extend(kids.get(&Some(id)).into_iter().flatten().rev().map(|(c, n)| (depth + 1, *c, n.clone())));
        out.push((depth, name));
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::MAIN_SEPARATOR as SEP;
    use std::path::Path;

    use chrono::{TimeZone, Utc};

    use super::{ancestors, children, dir_paths, parent_path, search_catalog, subtree_dirs};
    use crate::catalog::scan::{ScanControl, ScanOptions, catalog};
    use crate::catalog::store::{CatalogDir, CatalogInfo, open_read};
    use crate::core::model::{Entry, KindFilter, SearchQuery};
    use crate::core::pattern::Expr;

    /// Catalog of a source tree given as (relative path, size; `None` = folder).
    pub(crate) fn catalog_of(name: &str, files: &[(&str, Option<usize>)]) -> (tempfile::TempDir, CatalogInfo) {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        for (path, size) in files {
            let full = src.join(path);
            match size {
                None => std::fs::create_dir_all(&full).unwrap(),
                Some(n) => {
                    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
                    std::fs::write(&full, vec![b'x'; *n]).unwrap();
                }
            }
        }
        let dir = CatalogDir::new(tmp.path().to_path_buf());
        let opts = ScanOptions { name: name.to_owned(), ..ScanOptions::default() };
        let info = catalog(&dir, &src, &opts, &ScanControl::default()).unwrap();
        (tmp, info)
    }

    fn tree() -> (tempfile::TempDir, CatalogInfo) {
        catalog_of(
            "T",
            &[
                ("BD/Astérix/01.cbz", Some(30)),
                ("BD/Astérix/02.cbz", Some(10)),
                ("BD/zz.txt", Some(1)),
                ("Films/Asterix film.mkv", Some(200)),
                ("100%.txt", Some(2)),
                ("my_file.txt", Some(2)),
                ("myxfile.txt", Some(2)),
                ("Empty", None),
            ],
        )
    }

    fn q(pattern: &str) -> SearchQuery {
        SearchQuery { pattern: Expr::parse(pattern).unwrap(), ..SearchQuery::all() }
    }

    fn names(hits: &[crate::core::model::Hit]) -> Vec<String> {
        let mut n: Vec<String> = hits.iter().map(|h| h.entry.name.clone()).collect();
        n.sort();
        n
    }

    fn find(conn: &rusqlite::Connection, name: &str) -> Entry {
        let id: i64 = conn.query_row("SELECT id FROM entries WHERE name = ?1", [name], |r| r.get(0)).unwrap();
        ancestors(conn, id).unwrap().pop().unwrap()
    }

    #[test]
    fn children_folders_first() {
        let (_t, info) = tree();
        let conn = open_read(&info.file).unwrap();
        let top: Vec<String> = children(&conn, None).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(top, ["BD", "Empty", "Films", "100%.txt", "my_file.txt", "myxfile.txt"]);
        let bd = find(&conn, "BD");
        let inside: Vec<(String, bool, Option<u64>)> =
            children(&conn, Some(bd.id)).unwrap().into_iter().map(|e| (e.name, e.is_dir, e.size)).collect();
        assert_eq!(inside, [("Astérix".to_owned(), true, None), ("zz.txt".to_owned(), false, Some(1))]);
    }

    #[test]
    fn ancestors_root_to_leaf() {
        let (_t, info) = tree();
        let conn = open_read(&info.file).unwrap();
        let leaf = find(&conn, "01.cbz");
        let chain: Vec<String> = ancestors(&conn, leaf.id).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(chain, ["BD", "Astérix", "01.cbz"]);
    }

    #[test]
    fn dir_paths_leading_separator() {
        let (_t, info) = tree();
        let conn = open_read(&info.file).unwrap();
        let paths = dir_paths(&conn).unwrap();
        let asterix = find(&conn, "Astérix");
        assert_eq!(paths[&asterix.id], format!("{SEP}BD{SEP}Astérix"));
        assert_eq!(parent_path(&paths, Some(asterix.id)), format!("{SEP}BD{SEP}Astérix"));
        assert_eq!(parent_path(&paths, None), SEP.to_string());
    }

    #[test]
    fn search_pattern_folded() {
        let (_t, info) = tree();
        let conn = open_read(&info.file).unwrap();
        let hits = search_catalog(&conn, &q("ASTERIX"), &info, None).unwrap();
        assert_eq!(names(&hits), ["Asterix film.mkv", "Astérix"]);
        assert_eq!(hits[0].catalog_name, "T");
        assert_eq!(hits[0].catalog_file, info.file);
    }

    #[test]
    fn search_literal_percent_underscore() {
        let (_t, info) = tree();
        let conn = open_read(&info.file).unwrap();
        assert_eq!(names(&search_catalog(&conn, &q("100%"), &info, None).unwrap()), ["100%.txt"]);
        assert_eq!(names(&search_catalog(&conn, &q("my_file"), &info, None).unwrap()), ["my_file.txt"]);
    }

    #[test]
    fn search_expression() {
        let (_t, info) = tree();
        let conn = open_read(&info.file).unwrap();
        let found = |pattern: &str| names(&search_catalog(&conn, &q(pattern), &info, None).unwrap());
        assert_eq!(found("zz OR 100%"), ["100%.txt", "zz.txt"]);
        assert_eq!(found("asterix AND .mkv|"), ["Asterix film.mkv"]);
        assert_eq!(found("(01 OR 02 OR film) AND |ast"), ["Asterix film.mkv"]);
        assert_eq!(found("(01 OR 02 OR film) AND .cbz|"), ["01.cbz", "02.cbz"]);
        let parents = SearchQuery { parents: true, ..q("|aster AND ix|") };
        assert_eq!(names(&search_catalog(&conn, &parents, &info, None).unwrap()), ["01.cbz", "02.cbz"]);
        let parents = SearchQuery { parents: true, ..q("asterix OR films") };
        assert_eq!(
            names(&search_catalog(&conn, &parents, &info, None).unwrap()),
            ["01.cbz", "02.cbz", "Asterix film.mkv"]
        );
    }

    #[test]
    fn search_parents_mode() {
        let (_t, info) = tree();
        let conn = open_read(&info.file).unwrap();
        let query = SearchQuery { parents: true, ..q("asterix") };
        assert_eq!(names(&search_catalog(&conn, &query, &info, None).unwrap()), ["01.cbz", "02.cbz"]);
    }

    #[test]
    fn search_size_excludes_dirs() {
        let (_t, info) = tree();
        let conn = open_read(&info.file).unwrap();
        let query = SearchQuery { min_size: Some(10), max_size: Some(30), ..q("") };
        assert_eq!(names(&search_catalog(&conn, &query, &info, None).unwrap()), ["01.cbz", "02.cbz"]);
    }

    #[test]
    fn search_date_bounds() {
        let (t, _) = tree();
        let old = Utc.with_ymd_and_hms(2019, 3, 2, 12, 0, 0).unwrap();
        let file = std::fs::File::options().write(true).open(t.path().join("src/BD/zz.txt")).unwrap();
        file.set_modified(old.into()).unwrap();
        drop(file);
        // Re-catalog so the new date is recorded.
        let dir = CatalogDir::new(t.path().to_path_buf());
        let opts = ScanOptions { name: "T".to_owned(), ..ScanOptions::default() };
        let info = catalog(&dir, &t.path().join("src"), &opts, &ScanControl::default()).unwrap();
        let conn = open_read(&info.file).unwrap();
        let day = old.timestamp();
        let before = SearchQuery { to: Some(day + 1), ..q("") };
        assert_eq!(names(&search_catalog(&conn, &before, &info, None).unwrap()), ["zz.txt"]);
        let after = SearchQuery { from: Some(day + 1), kind: KindFilter::Files, ..q("zz") };
        assert_eq!(names(&search_catalog(&conn, &after, &info, None).unwrap()), [] as [&str; 0]);
    }

    #[test]
    fn search_kind_dirs() {
        let (_t, info) = tree();
        let conn = open_read(&info.file).unwrap();
        let query = SearchQuery { kind: KindFilter::Dirs, ..q("") };
        assert_eq!(names(&search_catalog(&conn, &query, &info, None).unwrap()), ["Astérix", "BD", "Empty", "Films"]);
        let files = SearchQuery { kind: KindFilter::Files, ..q("asterix") };
        assert_eq!(names(&search_catalog(&conn, &files, &info, None).unwrap()), ["Asterix film.mkv"]);
    }

    #[test]
    fn limit_respected() {
        let (_t, info) = tree();
        let conn = open_read(&info.file).unwrap();
        assert_eq!(search_catalog(&conn, &q(""), &info, Some(3)).unwrap().len(), 3);
    }

    #[test]
    fn subtree_depth_first() {
        let (_t, info) = catalog_of("T", &[("A/B/C", None), ("A/a2", None), ("Z", None), ("A/f.txt", Some(1))]);
        let conn = open_read(&info.file).unwrap();
        let all = subtree_dirs(&conn, None).unwrap();
        let expected: Vec<(usize, String)> =
            [(0, "A"), (1, "a2"), (1, "B"), (2, "C"), (0, "Z")].iter().map(|(d, n)| (*d, (*n).to_owned())).collect();
        assert_eq!(all, expected);
        let a = find(&conn, "A");
        assert_eq!(subtree_dirs(&conn, Some(a.id)).unwrap()[0], (0, "a2".to_owned()));
        assert!(Path::new(&info.file).exists());
    }
}
