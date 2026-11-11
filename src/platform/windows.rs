//! Windows implementation (Win32).

use std::ffi::OsStr;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW, GetVolumePathNameW,
};
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows_sys::Win32::System::Diagnostics::Debug::{SEM_FAILCRITICALERRORS, SEM_NOOPENFILEERRORBOX, SetErrorMode};
use windows_sys::Win32::UI::Shell::{SEE_MASK_INVOKEIDLIST, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOW;

use super::VolumeInfo;

const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_CDROM: u32 = 5;
const MAX_PATH_BUF: usize = 32_768;

/// NUL-terminated UTF-16 copy of an OS string.
fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// String up to the first NUL of a UTF-16 buffer.
fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    std::ffi::OsString::from_wide(&buf[..end]).to_string_lossy().into_owned()
}

/// Root (`E:\`, `\\server\share\`) of the volume holding a path.
fn volume_root(path: &Path) -> Option<PathBuf> {
    let absolute = std::path::absolute(path).ok()?;
    let input = wide(absolute.as_os_str());
    let mut buf = vec![0u16; MAX_PATH_BUF];
    // SAFETY: `input` is NUL-terminated and `buf` is writable for the length passed.
    let ok = unsafe { GetVolumePathNameW(input.as_ptr(), buf.as_mut_ptr(), u32::try_from(buf.len()).ok()?) };
    (ok != 0).then(|| PathBuf::from(from_wide(&buf)))
}

/// Label, serial and filesystem of a volume root.
fn volume_identity(root: &Path) -> (Option<String>, Option<String>, Option<String>) {
    let root_w = wide(root.as_os_str());
    let mut label = [0u16; 261];
    let mut fs = [0u16; 261];
    let mut serial = 0u32;
    // SAFETY: buffers are writable for the lengths passed; unused out-pointers are null.
    let ok = unsafe {
        GetVolumeInformationW(
            root_w.as_ptr(),
            label.as_mut_ptr(),
            261,
            &raw mut serial,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            fs.as_mut_ptr(),
            261,
        )
    };
    if ok == 0 {
        return (None, None, None);
    }
    let label = Some(from_wide(&label)).filter(|l| !l.is_empty());
    let serial = Some(format!("{:04X}-{:04X}", serial >> 16, serial & 0xFFFF));
    (label, serial, Some(from_wide(&fs)).filter(|f| !f.is_empty()))
}

pub(super) fn volume_info(path: &Path) -> VolumeInfo {
    let Some(root) = volume_root(path) else {
        return VolumeInfo::default();
    };
    let (label, serial, filesystem) = volume_identity(&root);
    let root_w = wide(root.as_os_str());
    let (mut free, mut total) = (0u64, 0u64);
    // SAFETY: `root_w` is NUL-terminated; out-pointers are valid or null.
    let sized = unsafe { GetDiskFreeSpaceExW(root_w.as_ptr(), &raw mut free, &raw mut total, std::ptr::null_mut()) };
    // SAFETY: `root_w` is NUL-terminated.
    let drive_type = unsafe { GetDriveTypeW(root_w.as_ptr()) };
    VolumeInfo {
        serial,
        label,
        filesystem,
        root: Some(root),
        total_bytes: (sized != 0).then_some(total),
        free_bytes: (sized != 0).then_some(free),
        removable: matches!(drive_type, DRIVE_REMOVABLE | DRIVE_CDROM),
    }
}

pub(super) fn mounted_volumes() -> Vec<(PathBuf, Option<String>)> {
    // SAFETY: no arguments.
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| {
            let root = PathBuf::from(format!("{}:\\", char::from(b'A' + i)));
            let serial = volume_identity(&root).1;
            (root, serial)
        })
        .collect()
}

pub(super) fn show_properties(path: &Path) -> anyhow::Result<()> {
    let file = wide(path.as_os_str());
    let verb = wide(OsStr::new("properties"));
    // SAFETY: SHELLEXECUTEINFOW is a plain C struct for which all-zero is a valid initial value.
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = u32::try_from(std::mem::size_of::<SHELLEXECUTEINFOW>())?;
    info.fMask = SEE_MASK_INVOKEIDLIST;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.nShow = SW_SHOW;
    // SAFETY: `info` is initialized and its string pointers outlive the call.
    if unsafe { ShellExecuteExW(&raw mut info) } == 0 {
        anyhow::bail!("cannot show the properties of {}", path.display());
    }
    Ok(())
}

pub(super) fn attach_parent_console() {
    // SAFETY: no pointers; failure (no parent console) is harmless.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

pub(super) fn quiet_drive_errors() {
    // SAFETY: plain flag setter.
    unsafe {
        SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX);
    }
}
