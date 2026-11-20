//! Crash log: panics are appended to a file, since the Windows executable usually has no console to print them to.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Name of the crash log, stored in the catalog directory.
pub const LOG_FILE: &str = "oxycat-crash.log";

/// Makes every panic append a report to `log`, then report it as usual (stderr).
pub fn install(log: PathBuf) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let report = format!(
            "{} oxycat {}, thread '{}': {info}",
            jiff::Timestamp::now(),
            env!("CARGO_PKG_VERSION"),
            thread.name().unwrap_or("unnamed"),
        );
        // Best effort: an unwritable log must not hide the panic itself.
        let _ = append(&log, &report);
        default(info);
    }));
}

/// Appends `report` and a line break to `log`, creating the file when needed.
fn append(log: &Path, report: &str) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(log)?;
    writeln!(file, "{report}")
}

#[cfg(test)]
mod tests {
    use super::{LOG_FILE, install};

    #[test]
    fn panics_are_logged() {
        let tmp = tempfile::tempdir().unwrap();
        let log = tmp.path().join(LOG_FILE);
        install(log.clone());
        let crashed = std::thread::Builder::new().name("doomed".to_owned()).spawn(|| panic!("boom")).unwrap().join();
        // Back to the default hook, so that this test leaves no trace on the others.
        drop(std::panic::take_hook());
        assert!(crashed.is_err());
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.contains("thread 'doomed'") && text.contains("boom"), "{text:?}");
    }
}
