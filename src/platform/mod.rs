//! Operating-system specific calls: volume information, mounted volumes, opening files.

use std::path::{Path, PathBuf};

pub mod mountinfo;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as os;
#[cfg(windows)]
use windows as os;

/// What the OS tells about the volume holding a path. Every field is best effort.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VolumeInfo {
    /// Serial number (Windows `XXXX-XXXX`) or filesystem UUID (Linux).
    pub serial: Option<String>,
    /// Volume label.
    pub label: Option<String>,
    /// Filesystem type.
    pub filesystem: Option<String>,
    /// Root of the volume (`E:\`, mount point).
    pub root: Option<PathBuf>,
    /// Capacity in bytes.
    pub total_bytes: Option<u64>,
    /// Free space in bytes.
    pub free_bytes: Option<u64>,
    /// Removable media (USB key, card, optical disc).
    pub removable: bool,
}

/// Information about the volume holding `path`. Never fails: unknown fields stay `None`.
#[must_use]
pub fn volume_info(path: &Path) -> VolumeInfo {
    os::volume_info(path)
}

/// Mounted volumes: root and serial number.
#[must_use]
pub fn mounted_volumes() -> Vec<(PathBuf, Option<String>)> {
    os::mounted_volumes()
}

/// Opens a file or folder with its default application.
///
/// # Errors
/// When the OS cannot open it.
pub fn open(path: &Path) -> anyhow::Result<()> {
    Ok(opener::open(path)?)
}

/// Shows a file or folder in the file manager.
///
/// # Errors
/// When the OS cannot show it.
pub fn reveal(path: &Path) -> anyhow::Result<()> {
    Ok(opener::reveal(path)?)
}

/// An icon as RGBA pixels (not premultiplied), rows top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Icon {
    /// Width and height in pixels.
    pub size: [usize; 2],
    /// `size[0] * size[1] * 4` bytes.
    pub rgba: Vec<u8>,
}

/// The icon the OS shows for files named like `name`, looked up by extension: the file does not need to exist.
/// `large` asks for the 32 px icon instead of the 16 px one (for high-DPI screens). `None` when unavailable,
/// always on Linux (no simple system-wide API for it).
#[must_use]
pub fn file_icon(name: &str, large: bool) -> Option<Icon> {
    #[cfg(windows)]
    {
        os::file_icon(name, large)
    }
    #[cfg(not(windows))]
    {
        let _ = (name, large);
        None
    }
}

/// Whether [`show_properties`] is available on this OS.
pub const HAS_PROPERTIES: bool = cfg!(windows);

/// Opens the OS properties dialog of a file or folder (Windows only).
///
/// # Errors
/// When the dialog cannot be shown, or on other platforms.
pub fn show_properties(path: &Path) -> anyhow::Result<()> {
    os::show_properties(path)
}

/// Stops Windows from showing "no disk in drive" dialogs when an empty card reader or optical drive is probed.
pub fn quiet_drive_errors() {
    #[cfg(windows)]
    os::quiet_drive_errors();
}

/// Attaches to the console of the parent process, so a GUI-subsystem executable can print when run from a terminal.
pub fn attach_parent_console() {
    #[cfg(windows)]
    os::attach_parent_console();
}

#[cfg(test)]
mod tests {
    use super::volume_info;

    #[cfg(windows)]
    #[test]
    fn file_icon_by_extension() {
        // The file does not exist: the icon comes from the extension's registration.
        for (large, side) in [(false, 16), (true, 32)] {
            let icon = super::file_icon("missing.txt", large).unwrap();
            assert_eq!(icon.size, [side, side]);
            assert_eq!(icon.rgba.len(), side * side * 4);
            assert!(icon.rgba.chunks(4).any(|p| p[3] > 0), "fully transparent icon");
        }
    }

    #[test]
    fn volume_info_of_tempdir_has_total() {
        let tmp = tempfile::tempdir().unwrap();
        let info = volume_info(tmp.path());
        assert!(info.total_bytes.is_some_and(|t| t > 0), "{info:?}");
        assert!(info.root.is_some(), "{info:?}");
    }
}
