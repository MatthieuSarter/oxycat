//! Oxycat entry point: the graphical interface, or the command line when a command is given.

// A GUI-subsystem executable shows no console window when started from Explorer; with arguments it attaches to the
// parent console to print.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;

use oxycat::catalog::store::CatalogDir;
use oxycat::cli::{Args, Command, run_catalog};
use oxycat::config::{CATALOGS_ENV, load_settings, resolve_catalog_dir};
use oxycat::i18n::detect;
use oxycat::platform;

fn main() -> ExitCode {
    platform::quiet_drive_errors();
    if std::env::args_os().len() > 1 {
        platform::attach_parent_console();
    }
    let args = match Args::try_parse() {
        Ok(args) => args,
        Err(e) => {
            let _ = e.print();
            return ExitCode::from(if e.use_stderr() { 2 } else { 0 });
        }
    };
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    let dir =
        resolve_catalog_dir(args.catalogs, std::env::var_os(CATALOGS_ENV), &exe_dir.unwrap_or(PathBuf::from(".")));
    let (settings, warning) = load_settings(&dir);
    let lang = settings.language.unwrap_or_else(|| detect(sys_locale::get_locale().as_deref()));
    match args.command {
        Some(Command::Catalog(catalog)) => {
            if let Some(warning) = warning {
                eprintln!("{warning}");
            }
            match run_catalog(&CatalogDir::new(dir), &catalog, &settings.default_ignore, lang) {
                Ok(message) => {
                    println!("{message}");
                    ExitCode::SUCCESS
                }
                Err(message) => {
                    eprintln!("{message}");
                    ExitCode::FAILURE
                }
            }
        }
        None => {
            let _ = std::fs::create_dir_all(&dir);
            match oxycat::gui::run(CatalogDir::new(dir), settings, warning) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
