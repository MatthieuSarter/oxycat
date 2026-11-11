//! End-to-end tests of the `oxycat catalog` command line.

use std::path::Path;
use std::process::{Command, Output};

use oxycat::catalog::store::{CatalogDir, MetaUpdate};

fn oxycat(args: &[&str], env: Option<&Path>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_oxycat"));
    cmd.args(args).env_remove("OXYCAT_CATALOGS");
    if let Some(dir) = env {
        cmd.env("OXYCAT_CATALOGS", dir);
    }
    cmd.output().unwrap()
}

struct Dirs {
    _tmp: tempfile::TempDir,
    src: String,
    other: String,
    cats: String,
}

fn dirs() -> Dirs {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    std::fs::create_dir_all(src.join("BD")).unwrap();
    std::fs::write(src.join("BD").join("01.cbz"), b"abc").unwrap();
    let other = tmp.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    let cats = tmp.path().join("cats");
    let s = |p: &Path| p.to_string_lossy().into_owned();
    Dirs { src: s(&src), other: s(&other), cats: s(&cats), _tmp: tmp }
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn creates_catalog() {
    let d = dirs();
    let out = oxycat(&["--catalogs", &d.cats, "catalog", &d.src, "--name", "T", "--comment", "bleu"], None);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let info = CatalogDir::new(d.cats.into()).find("T").unwrap();
    assert_eq!((info.volume.name.as_str(), info.volume.comment.as_str()), ("T", "bleu"));
    assert_eq!((info.volume.file_count, info.volume.dir_count), (1, 1));
}

#[test]
fn rerun_is_refresh() {
    let d = dirs();
    assert_eq!(oxycat(&["catalog", &d.src, "--name", "T", "--catalogs", &d.cats], None).status.code(), Some(0));
    let out = oxycat(&["catalog", &d.src, "--name", "T", "--catalogs", &d.cats], None);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
}

#[test]
fn other_source_same_name_needs_overwrite() {
    let d = dirs();
    oxycat(&["catalog", &d.src, "--name", "T", "--catalogs", &d.cats], None);
    let refused = oxycat(&["catalog", &d.other, "--name", "T", "--catalogs", &d.cats], None);
    assert_eq!(refused.status.code(), Some(1));
    assert!(stderr(&refused).contains("--overwrite"), "{}", stderr(&refused));
    let forced = oxycat(&["catalog", &d.other, "--name", "T", "--overwrite", "--catalogs", &d.cats], None);
    assert_eq!(forced.status.code(), Some(0), "{}", stderr(&forced));
}

#[test]
fn preserved_refused() {
    let d = dirs();
    oxycat(&["catalog", &d.src, "--name", "T", "--catalogs", &d.cats], None);
    let dir = CatalogDir::new(d.cats.clone().into());
    let file = dir.find("T").unwrap().file;
    dir.update_meta(&file, &MetaUpdate { comment: String::new(), archive: false, preserved: true }).unwrap();
    let out = oxycat(&["catalog", &d.src, "--name", "T", "--overwrite", "--catalogs", &d.cats], None);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn missing_source_fails() {
    let d = dirs();
    let out = oxycat(&["catalog", &format!("{}-nope", d.src), "--name", "T", "--catalogs", &d.cats], None);
    assert_eq!(out.status.code(), Some(1));
    assert!(!stderr(&out).is_empty());
}

#[test]
fn env_var_catalog_dir() {
    let d = dirs();
    let out = oxycat(&["catalog", &d.src, "--name", "T"], Some(Path::new(&d.cats)));
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(CatalogDir::new(d.cats.into()).find("T").is_some());
}

#[test]
fn bad_args_exit_2() {
    assert_eq!(oxycat(&["catalog"], None).status.code(), Some(2));
    let d = dirs();
    let both = oxycat(&["catalog", &d.src, "--name", "T", "--archive", "--no-archive", "--catalogs", &d.cats], None);
    assert_eq!(both.status.code(), Some(2));
}

#[test]
fn archive_flag_and_ignore() {
    let d = dirs();
    let args = ["catalog", &d.src, "--name", "T", "--archive", "--ignore", "*.cbz", "--catalogs", &d.cats];
    assert_eq!(oxycat(&args, None).status.code(), Some(0));
    let info = CatalogDir::new(d.cats.into()).find("T").unwrap();
    assert!(info.volume.archive);
    assert_eq!(info.volume.file_count, 0);
}
