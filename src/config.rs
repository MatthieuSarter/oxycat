//! Catalog directory resolution and the `oxycat.toml` settings file.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::Lang;

/// Name of the settings file, stored in the catalog directory.
pub const SETTINGS_FILE: &str = "oxycat.toml";

/// Environment variable naming the catalog directory.
pub const CATALOGS_ENV: &str = "OXYCAT_CATALOGS";

/// Picks the catalog directory: the `--catalogs` option, else a non-empty `OXYCAT_CATALOGS`, else the executable's
/// directory.
#[must_use]
pub fn resolve_catalog_dir(cli: Option<PathBuf>, env: Option<OsString>, exe_dir: &Path) -> PathBuf {
    cli.or_else(|| env.filter(|e| !e.is_empty()).map(PathBuf::from)).unwrap_or_else(|| exe_dir.to_path_buf())
}

/// The interface theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    /// Always light.
    Light,
    /// Always dark.
    Dark,
    /// Follow the operating system.
    #[default]
    Auto,
}

/// User settings persisted in `oxycat.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Interface theme.
    pub theme: ThemeChoice,
    /// Interface language; `None` follows the OS locale.
    pub language: Option<Lang>,
    /// Maximum number of search results.
    pub hit_limit: usize,
    /// Ignore patterns proposed when cataloging (Cathy syntax).
    pub default_ignore: String,
    /// `strftime`-style date-time format overriding the language's default.
    pub date_format: Option<String>,
    /// Thousands separator overriding the language's default.
    pub thousands_separator: Option<String>,
    /// Whether the catalog panel is collapsed to the bare list.
    pub catalogs_collapsed: bool,
    /// Whether search results identical but for their volume are shown as a single row.
    pub group_identical: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: ThemeChoice::Auto,
            language: None,
            hit_limit: 10_000,
            default_ignore: String::new(),
            date_format: None,
            thousands_separator: None,
            catalogs_collapsed: false,
            group_identical: true,
        }
    }
}

/// Loads the settings of a catalog directory. A missing file gives the defaults silently; an unreadable or invalid
/// one gives the defaults plus a warning to show the user.
#[must_use]
pub fn load_settings(dir: &Path) -> (Settings, Option<String>) {
    let path = dir.join(SETTINGS_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Settings::default(), None),
        Err(e) => return (Settings::default(), Some(format!("{}: {e}", path.display()))),
    };
    match toml::from_str(&text) {
        Ok(settings) => (settings, None),
        Err(e) => (Settings::default(), Some(format!("{}: {e}", path.display()))),
    }
}

/// Writes the settings to the catalog directory, through a temporary file renamed over the old one so that a crash
/// mid-write never leaves a truncated file.
///
/// # Errors
/// Any I/O error, e.g. a read-only catalog directory.
pub fn save_settings(dir: &Path, settings: &Settings) -> std::io::Result<()> {
    let text = toml::to_string(settings).map_err(std::io::Error::other)?;
    let tmp = dir.join(format!("{SETTINGS_FILE}.tmp"));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, dir.join(SETTINGS_FILE)).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use super::{SETTINGS_FILE, Settings, ThemeChoice, load_settings, resolve_catalog_dir, save_settings};
    use crate::i18n::Lang;

    const EXE: &str = "/opt/oxycat";

    #[test]
    fn cli_wins() {
        let dir = resolve_catalog_dir(Some(PathBuf::from("/cli")), Some(OsString::from("/env")), Path::new(EXE));
        assert_eq!(dir, PathBuf::from("/cli"));
    }

    #[test]
    fn env_second() {
        let dir = resolve_catalog_dir(None, Some(OsString::from("/env")), Path::new(EXE));
        assert_eq!(dir, PathBuf::from("/env"));
    }

    #[test]
    fn empty_env_ignored() {
        assert_eq!(resolve_catalog_dir(None, Some(OsString::new()), Path::new(EXE)), PathBuf::from(EXE));
    }

    #[test]
    fn exe_dir_default() {
        assert_eq!(resolve_catalog_dir(None, None, Path::new(EXE)), PathBuf::from(EXE));
    }

    #[test]
    fn missing_file_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let (settings, warning) = load_settings(tmp.path());
        assert_eq!(settings.hit_limit, 10_000);
        assert_eq!(settings.theme, ThemeChoice::Auto);
        assert!(warning.is_none());
    }

    #[test]
    fn invalid_toml_warns() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(SETTINGS_FILE), "theme = 3").unwrap();
        let (settings, warning) = load_settings(tmp.path());
        assert_eq!(settings, Settings::default());
        assert!(warning.is_some());
    }

    #[test]
    fn partial_file_keeps_other_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(SETTINGS_FILE), "theme = \"dark\"\n").unwrap();
        let (settings, warning) = load_settings(tmp.path());
        assert_eq!(settings.theme, ThemeChoice::Dark);
        assert_eq!(settings.hit_limit, 10_000);
        assert!(warning.is_none());
    }

    #[test]
    fn roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let settings = Settings {
            theme: ThemeChoice::Light,
            language: Some(Lang::Es),
            hit_limit: 500,
            default_ignore: "*.tmp".to_owned(),
            date_format: Some("%Y".to_owned()),
            thousands_separator: Some(" ".to_owned()),
            catalogs_collapsed: true,
            group_identical: false,
        };
        save_settings(tmp.path(), &settings).unwrap();
        assert_eq!(load_settings(tmp.path()), (settings, None));
        // Saving again replaces the file and leaves no temporary file behind.
        save_settings(tmp.path(), &Settings::default()).unwrap();
        assert_eq!(load_settings(tmp.path()), (Settings::default(), None));
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
    }

    #[test]
    fn save_fails_in_missing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(save_settings(&tmp.path().join("missing"), &Settings::default()).is_err());
    }
}
