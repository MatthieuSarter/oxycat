//! What writing a catalog under an existing name means: create, refresh or overwrite.

use super::locate::source_rel;
use super::model::Volume;

/// How a scan result is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteDecision {
    /// No catalog with that name yet.
    Create,
    /// Same medium and source: replace the entries, keep name, comment and flags.
    Refresh,
    /// Another medium or source, explicitly allowed: replace everything.
    Overwrite,
}

/// Why a catalog cannot be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecideError {
    /// The existing catalog is preserved: never overwritten nor refreshed.
    Preserved,
    /// The name belongs to another medium or source, and overwriting was not allowed.
    NameTaken,
}

/// Decides how to write a new scan of `new_source_rel` (relative to its volume root, see [`source_rel`]) on the
/// volume with serial `new_serial`, given the catalog already using that name, if any.
///
/// # Errors
/// [`DecideError::Preserved`] or [`DecideError::NameTaken`].
pub fn decide(
    existing: Option<&Volume>,
    new_serial: Option<&str>,
    new_source_rel: &str,
    overwrite: bool,
) -> Result<WriteDecision, DecideError> {
    let Some(existing) = existing else {
        return Ok(WriteDecision::Create);
    };
    if existing.preserved {
        return Err(DecideError::Preserved);
    }
    let trim = |p: &str| p.trim_end_matches(['\\', '/']).to_lowercase();
    let old_rel = source_rel(&existing.source_path, existing.volume_root.as_deref());
    let same_medium = existing.serial.as_deref() == new_serial && trim(&old_rel) == trim(new_source_rel);
    match (same_medium, overwrite) {
        (true, _) => Ok(WriteDecision::Refresh),
        (false, true) => Ok(WriteDecision::Overwrite),
        (false, false) => Err(DecideError::NameTaken),
    }
}

#[cfg(test)]
mod tests {
    use super::{DecideError, WriteDecision, decide};
    use crate::catalog::store::tests::sample_volume;

    #[test]
    fn decisions() {
        let mut v = sample_volume("B1");
        v.source_path = "E:\\BD".to_owned();
        let serial = v.serial.clone();
        let s = serial.as_deref();
        assert_eq!(decide(None, s, "\\BD", false), Ok(WriteDecision::Create));
        assert_eq!(decide(Some(&v), s, "\\BD", false), Ok(WriteDecision::Refresh));
        assert_eq!(decide(Some(&v), s, "\\bd\\", false), Ok(WriteDecision::Refresh));
        assert_eq!(decide(Some(&v), s, "\\Films", false), Err(DecideError::NameTaken));
        assert_eq!(decide(Some(&v), Some("0000-0000"), "\\BD", false), Err(DecideError::NameTaken));
        assert_eq!(decide(Some(&v), None, "\\BD", false), Err(DecideError::NameTaken));
        assert_eq!(decide(Some(&v), Some("0000-0000"), "\\BD", true), Ok(WriteDecision::Overwrite));
        v.preserved = true;
        assert_eq!(decide(Some(&v), s, "\\BD", true), Err(DecideError::Preserved));
    }

    #[test]
    fn serial_less_media_compare_paths() {
        let mut v = sample_volume("NAS");
        v.serial = None;
        v.volume_root = None;
        v.source_path = "\\\\nas\\share".to_owned();
        assert_eq!(decide(Some(&v), None, "\\\\NAS\\share", false), Ok(WriteDecision::Refresh));
        assert_eq!(decide(Some(&v), None, "\\\\nas\\other", false), Err(DecideError::NameTaken));
    }
}
