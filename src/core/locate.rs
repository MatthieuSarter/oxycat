//! Finding where a cataloged medium is mounted now, and real paths of its entries.

use std::path::{Path, PathBuf};

use super::model::Volume;

/// The separator a stored path uses: `\` when it contains one, `/` otherwise.
fn separator_of(path: &str) -> char {
    if path.contains('\\') { '\\' } else { '/' }
}

/// Path of the cataloged source relative to its volume root, starting with the separator (`\BD` for `E:\BD` on
/// `E:\`). Without a known root, or when the root is not a prefix, the whole source path is returned.
#[must_use]
pub fn source_rel(source_path: &str, volume_root: Option<&str>) -> String {
    let sep = separator_of(source_path);
    let rest = volume_root.and_then(|root| {
        let rest = strip_prefix_ignore_case(source_path, root)?;
        // `/media/usb` is not a prefix of `/media/usb2`: the root must end at a separator.
        let at_boundary = root.ends_with(['\\', '/']) || rest.is_empty() || rest.starts_with(['\\', '/']);
        at_boundary.then_some(rest)
    });
    match rest {
        Some(rest) => {
            let rest = rest.trim_matches(['\\', '/']);
            if rest.is_empty() { sep.to_string() } else { format!("{sep}{rest}") }
        }
        None => source_path.to_owned(),
    }
}

/// `text` without `prefix`, compared case-insensitively character by character.
fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let mut chars = text.char_indices();
    for p in prefix.chars() {
        let (_, t) = chars.next()?;
        if !t.to_lowercase().eq(p.to_lowercase()) {
            return None;
        }
    }
    Some(chars.next().map_or("", |(i, _)| &text[i..]))
}

/// Appends the components of a stored path (either separator) to `base`.
fn join_stored(base: &Path, stored: &str) -> PathBuf {
    stored.split(['\\', '/']).filter(|c| !c.is_empty()).fold(base.to_path_buf(), |path, c| path.join(c))
}

/// Where the cataloged source can be read now.
///
/// A volume with a serial number is looked up among the mounted volumes (so a disk that moved from `E:` to `F:` is
/// still found); only a volume without serial number (e.g. a network share) falls back to its original path, when
/// `exists` says it is there.
#[must_use]
pub fn locate_source(
    v: &Volume,
    mounted: &[(PathBuf, Option<String>)],
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    // Network shares are not among the mounted drives: they are found by their path.
    match &v.serial {
        Some(serial) if !is_network_path(&v.source_path) => mounted
            .iter()
            .find(|(_, s)| s.as_ref() == Some(serial))
            .map(|(root, _)| join_stored(root, &source_rel(&v.source_path, v.volume_root.as_deref()))),
        _ => Some(PathBuf::from(&v.source_path)).filter(|p| exists(p)),
    }
}

/// Whether a stored source path is a network share (`\\server\share`, `//server/share`).
#[must_use]
pub fn is_network_path(path: &str) -> bool {
    path.starts_with(r"\\") || path.starts_with("//")
}

/// Real path of an entry, from the located source root, the entry's parent folder path (as stored in search results:
/// `\BD\Asterix`, or a lone separator for the root) and its name.
#[must_use]
pub fn entry_path(source_root: &Path, parent_path: &str, name: &str) -> PathBuf {
    join_stored(source_root, parent_path).join(name)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{entry_path, is_network_path, locate_source, source_rel};
    use crate::catalog::store::tests::sample_volume;

    #[test]
    fn source_rel_strips_root() {
        assert_eq!(source_rel("E:\\BD\\", Some("E:\\")), "\\BD");
        assert_eq!(source_rel("e:\\bd", Some("E:\\")), "\\bd");
        assert_eq!(source_rel("E:\\", Some("E:\\")), "\\");
        assert_eq!(source_rel("/media/usb/BD", Some("/media/usb")), "/BD");
        assert_eq!(source_rel("\\\\nas\\share\\BD", None), "\\\\nas\\share\\BD");
        assert_eq!(source_rel("D:\\x", Some("E:\\")), "D:\\x");
    }

    #[test]
    fn moved_drive_found_by_serial() {
        let mut v = sample_volume("B1");
        v.source_path = "E:\\BD".to_owned();
        let mounted =
            [(PathBuf::from("C:\\"), Some("1111-2222".to_owned())), (PathBuf::from("F:\\"), v.serial.clone())];
        let found = locate_source(&v, &mounted, |_| true).unwrap();
        assert_eq!(found, Path::new("F:\\").join("BD"));
    }

    #[test]
    fn whole_volume_found_at_root() {
        let v = sample_volume("B1");
        let mounted = [(PathBuf::from("G:\\"), v.serial.clone())];
        assert_eq!(locate_source(&v, &mounted, |_| true).unwrap(), PathBuf::from("G:\\"));
    }

    #[test]
    fn serial_not_mounted_is_none_even_if_path_exists() {
        let v = sample_volume("B1");
        assert!(locate_source(&v, &[(PathBuf::from("E:\\"), Some("9999-9999".to_owned()))], |_| true).is_none());
    }

    #[test]
    fn network_share_found_by_path_even_with_serial() {
        let mut v = sample_volume("NAS");
        v.source_path = r"\\nas\photos".to_owned();
        v.volume_root = Some(r"\\nas\photos\".to_owned());
        assert_eq!(locate_source(&v, &[], |_| true), Some(PathBuf::from(r"\\nas\photos")));
        assert_eq!(locate_source(&v, &[], |_| false), None);
    }

    #[test]
    fn is_network_path_detection() {
        assert!(is_network_path(r"\\nas\share"));
        assert!(is_network_path("//nas/share"));
        assert!(!is_network_path(r"E:\BD"));
        assert!(!is_network_path("/media/usb"));
    }

    #[test]
    fn no_serial_falls_back_to_source_path() {
        let mut v = sample_volume("NAS");
        v.serial = None;
        v.source_path = "\\\\nas\\share".to_owned();
        assert_eq!(locate_source(&v, &[], |_| true), Some(PathBuf::from("\\\\nas\\share")));
        assert_eq!(locate_source(&v, &[], |_| false), None);
    }

    #[test]
    fn entry_paths() {
        let root = Path::new("F:\\");
        assert_eq!(entry_path(root, "\\BD\\Asterix", "01.cbz"), root.join("BD").join("Asterix").join("01.cbz"));
        assert_eq!(entry_path(root, "\\", "a.txt"), root.join("a.txt"));
        assert_eq!(entry_path(Path::new("/m"), "/BD", "x"), Path::new("/m").join("BD").join("x"));
    }
}
