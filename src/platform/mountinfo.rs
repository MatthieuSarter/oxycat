//! Parsing of Linux `/proc/self/mountinfo` (pure, compiled everywhere so it is tested on every platform).

use std::path::{Path, PathBuf};

/// One mount: mount point, source device and filesystem type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    /// Where it is mounted.
    pub point: PathBuf,
    /// Mount source, e.g. `/dev/sdb1`.
    pub device: String,
    /// Filesystem type, e.g. `ext4`.
    pub fstype: String,
}

/// Parses every line of a mountinfo file; malformed lines are skipped.
#[must_use]
pub fn parse(mountinfo: &str) -> Vec<Mount> {
    mountinfo
        .lines()
        .filter_map(|line| {
            let (left, right) = line.split_once(" - ")?;
            let point = left.split(' ').nth(4)?;
            let mut right = right.split(' ');
            let fstype = right.next()?;
            let device = right.next()?;
            Some(Mount { point: PathBuf::from(unescape(point)), device: unescape(device), fstype: fstype.to_owned() })
        })
        .collect()
}

/// The mount holding `path`: the one with the longest mount point that is a prefix of it.
#[must_use]
pub fn mount_for(path: &Path, mounts: &[Mount]) -> Option<Mount> {
    mounts.iter().filter(|m| path.starts_with(&m.point)).max_by_key(|m| m.point.components().count()).cloned()
}

/// Decodes the octal escapes (`\040` for a space) the kernel uses in mountinfo fields.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let octal =
            bytes.get(i + 1..i + 4).filter(|d| bytes[i] == b'\\' && d.iter().all(|b| (b'0'..=b'7').contains(b)));
        if let Some(digits) = octal {
            out.push(digits.iter().fold(0u8, |acc, d| acc.wrapping_mul(8).wrapping_add(d - b'0')));
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{mount_for, parse};

    const SAMPLE: &str = "\
22 1 8:2 / / rw,relatime shared:1 - ext4 /dev/sda2 rw
40 22 8:17 / /media/usb rw,nosuid shared:2 - exfat /dev/sdb1 rw
41 22 8:33 / /media/my\\040disk rw shared:3 - ntfs3 /dev/sdc1 rw
garbage line
";

    #[test]
    fn parses_lines() {
        let mounts = parse(SAMPLE);
        assert_eq!(mounts.len(), 3);
        assert_eq!(mounts[1].device, "/dev/sdb1");
        assert_eq!(mounts[1].fstype, "exfat");
        assert_eq!(mounts[2].point, PathBuf::from("/media/my disk"));
    }

    #[test]
    fn mount_for_longest_prefix() {
        let mounts = parse(SAMPLE);
        assert_eq!(mount_for(Path::new("/media/usb/BD"), &mounts).unwrap().device, "/dev/sdb1");
        assert_eq!(mount_for(Path::new("/media/usb2"), &mounts).unwrap().device, "/dev/sda2");
        assert_eq!(mount_for(Path::new("/home"), &mounts).unwrap().point, PathBuf::from("/"));
    }
}
