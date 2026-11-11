//! Command line interface: global options and the `catalog` command.

use std::path::PathBuf;

use clap::{Args as ClapArgs, Parser, Subcommand};

use crate::catalog::scan::{ScanControl, ScanError, ScanOptions, catalog};
use crate::catalog::store::CatalogDir;
use crate::core::ignore::IgnoreRules;
use crate::core::refresh::DecideError;
use crate::i18n::{Lang, fill, strings};

/// Oxycat: catalog storage media and search them offline. Without a command, opens the graphical interface.
#[derive(Debug, Parser)]
#[command(name = "oxycat", version)]
pub struct Args {
    /// Directory of the catalogs and of `oxycat.toml` (default: `OXYCAT_CATALOGS`, else the executable's directory).
    #[arg(long, global = true, value_name = "DIR")]
    pub catalogs: Option<PathBuf>,
    /// Command to run instead of opening the graphical interface.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Command line commands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create or refresh the catalog of a drive or folder (e.g. from a scheduled task).
    Catalog(CatalogArgs),
}

/// Arguments of `oxycat catalog`.
#[derive(Debug, ClapArgs)]
pub struct CatalogArgs {
    /// Drive or folder to catalog.
    pub path: PathBuf,
    /// Catalog name.
    #[arg(long)]
    pub name: String,
    /// Comment (kept from the existing catalog on refresh when omitted).
    #[arg(long)]
    pub comment: Option<String>,
    /// Mark the catalog as an archive.
    #[arg(long, conflicts_with = "no_archive")]
    pub archive: bool,
    /// Mark the catalog as not an archive.
    #[arg(long)]
    pub no_archive: bool,
    /// Ignore patterns, e.g. "*.tmp; \Temp*" (default: `default_ignore` from `oxycat.toml`).
    #[arg(long)]
    pub ignore: Option<String>,
    /// Replace a catalog of the same name made from another medium or folder.
    #[arg(long)]
    pub overwrite: bool,
}

/// Runs `oxycat catalog`; returns the message for stdout, or the error message for stderr.
///
/// # Errors
/// The translated reason the catalog could not be written.
pub fn run_catalog(dir: &CatalogDir, args: &CatalogArgs, default_ignore: &str, lang: Lang) -> Result<String, String> {
    let s = strings(lang);
    std::fs::create_dir_all(dir.root()).map_err(|e| fill(s.err_other, &[("error", &e.to_string())]))?;
    let existed = dir.find(&args.name).is_some();
    let opts = ScanOptions {
        name: args.name.clone(),
        comment: args.comment.clone(),
        archive: match (args.archive, args.no_archive) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        },
        ignore: IgnoreRules::parse(args.ignore.as_deref().unwrap_or(default_ignore)),
        overwrite: args.overwrite,
    };
    match catalog(dir, &args.path, &opts, &ScanControl::default()) {
        Ok(info) => {
            let template = if existed { s.cli_refreshed } else { s.cli_created };
            let (files, dirs) = (info.volume.file_count.to_string(), info.volume.dir_count.to_string());
            Ok(fill(template, &[("name", &info.volume.name), ("files", &files), ("dirs", &dirs)]))
        }
        Err(e) => Err(scan_error_message(&e, &args.name, &args.path.to_string_lossy(), lang)),
    }
}

/// Translated message for a scan error.
#[must_use]
pub fn scan_error_message(e: &ScanError, name: &str, path: &str, lang: Lang) -> String {
    let s = strings(lang);
    match e {
        ScanError::SourceMissing => fill(s.err_source_missing, &[("path", path)]),
        ScanError::Decide(DecideError::Preserved) => fill(s.err_preserved, &[("name", name)]),
        ScanError::Decide(DecideError::NameTaken) => fill(s.cli_name_taken, &[("name", name)]),
        ScanError::Cancelled => s.err_cancelled.to_owned(),
        ScanError::InUse => fill(s.err_in_use, &[("name", name)]),
        ScanError::Other(e) => fill(s.err_other, &[("error", &format!("{e:#}"))]),
    }
}
