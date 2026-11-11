//! Linux (and other Unix) implementation: `/proc/self/mountinfo`, `/dev/disk/by-*`, `statvfs`, sysfs.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use super::VolumeInfo;
use super::mountinfo::{Mount, mount_for, parse};

/// Mounts of the current process, empty when `/proc` is unavailable.
fn mounts() -> Vec<Mount> {
    std::fs::read_to_string("/proc/self/mountinfo").map(|s| parse(&s)).unwrap_or_default()
}

/// Name of the `/dev/disk/<kind>/` link pointing at `device`, e.g. its UUID or label.
fn disk_link(kind: &str, device: &str) -> Option<String> {
    let target = std::fs::canonicalize(device).ok()?;
    std::fs::read_dir(Path::new("/dev/disk").join(kind)).ok()?.filter_map(Result::ok).find_map(|entry| {
        (std::fs::canonicalize(entry.path()).ok()? == target).then(|| entry.file_name().to_string_lossy().into_owned())
    })
}

/// Whether the block device (or the disk it is a partition of) is flagged removable in sysfs.
fn is_removable(device: &str) -> bool {
    let Some(name) = Path::new(device).file_name() else {
        return false;
    };
    let Ok(sys) = std::fs::canonicalize(Path::new("/sys/class/block").join(name)) else {
        return false;
    };
    let flag = |dir: &Path| std::fs::read_to_string(dir.join("removable")).is_ok_and(|s| s.trim() == "1");
    flag(&sys) || sys.parent().is_some_and(flag)
}

/// Capacity and free space (for unprivileged users) of the filesystem holding a path.
fn space(path: &Path) -> Option<(u64, u64)> {
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `statvfs` is plain data; all-zero is a valid initial value.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c_path` is NUL-terminated and `stat` is writable.
    if unsafe { libc::statvfs(c_path.as_ptr(), &raw mut stat) } != 0 {
        return None;
    }
    #[allow(clippy::useless_conversion, reason = "field widths differ between platforms")]
    let (blocks, avail, frsize) = (u64::from(stat.f_blocks), u64::from(stat.f_bavail), u64::from(stat.f_frsize));
    Some((blocks.saturating_mul(frsize), avail.saturating_mul(frsize)))
}

pub(super) fn volume_info(path: &Path) -> VolumeInfo {
    let absolute = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mount = mount_for(&absolute, &mounts());
    let (total, free) = space(&absolute).unzip();
    VolumeInfo {
        serial: mount.as_ref().and_then(|m| disk_link("by-uuid", &m.device)),
        label: mount.as_ref().and_then(|m| disk_link("by-label", &m.device)),
        filesystem: mount.as_ref().map(|m| m.fstype.clone()),
        removable: mount.as_ref().is_some_and(|m| is_removable(&m.device)),
        root: mount.map(|m| m.point).or_else(|| Some(PathBuf::from("/"))),
        total_bytes: total,
        free_bytes: free,
    }
}

pub(super) fn mounted_volumes() -> Vec<(PathBuf, Option<String>)> {
    mounts()
        .into_iter()
        .filter(|m| m.device.starts_with("/dev/"))
        .map(|m| {
            let serial = disk_link("by-uuid", &m.device);
            (m.point, serial)
        })
        .collect()
}

pub(super) fn show_properties(path: &Path) -> anyhow::Result<()> {
    anyhow::bail!("no properties dialog on this platform for {}", path.display())
}
