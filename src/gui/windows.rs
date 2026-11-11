//! Settings and About windows.

use chrono::Utc;

use super::app::OxycatApp;
use crate::config::Settings;
use crate::core::listing::format_timestamp;

/// Text fields of the settings window, validated on save.
#[derive(Debug, Clone)]
pub(super) struct SettingsDraft {
    hit_limit: String,
    default_ignore: String,
    date_format: String,
    thousands_separator: String,
    error: bool,
}

impl From<&Settings> for SettingsDraft {
    fn from(s: &Settings) -> Self {
        SettingsDraft {
            hit_limit: s.hit_limit.to_string(),
            default_ignore: s.default_ignore.clone(),
            date_format: s.date_format.clone().unwrap_or_default(),
            thousands_separator: s.thousands_separator.clone().unwrap_or_default(),
            error: false,
        }
    }
}

impl SettingsDraft {
    /// The settings this draft describes, or `None` when a value is invalid.
    fn apply(&self, base: &Settings) -> Option<Settings> {
        let hit_limit = self.hit_limit.trim().parse::<usize>().ok().filter(|n| *n > 0)?;
        let date_format = Some(self.date_format.trim().to_owned()).filter(|f| !f.is_empty());
        if date_format.as_deref().is_some_and(|f| format_timestamp(0, f, &Utc).is_empty()) {
            return None;
        }
        Some(Settings {
            hit_limit,
            default_ignore: self.default_ignore.trim().to_owned(),
            date_format,
            thousands_separator: Some(self.thousands_separator.clone()).filter(|t| !t.is_empty()),
            ..base.clone()
        })
    }
}

impl OxycatApp {
    pub(super) fn windows(&mut self, ctx: &egui::Context) {
        let s = self.fmt.s;
        if let Some(mut draft) = self.settings_window.take() {
            let mut open = true;
            let mut close = false;
            let mut save = false;
            egui::Window::new(s.settings).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                egui::Grid::new("settings_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label(s.hit_limit);
                    ui.text_edit_singleline(&mut draft.hit_limit);
                    ui.end_row();
                    ui.label(s.default_ignore);
                    ui.add(egui::TextEdit::singleline(&mut draft.default_ignore).hint_text(s.ignore_hint));
                    ui.end_row();
                    ui.label(s.date_format);
                    ui.add(egui::TextEdit::singleline(&mut draft.date_format).hint_text("%d/%m/%Y %H:%M"));
                    ui.end_row();
                    ui.label(s.thousands_separator);
                    ui.text_edit_singleline(&mut draft.thousands_separator);
                    ui.end_row();
                });
                if draft.error {
                    ui.colored_label(ui.visuals().error_fg_color, s.invalid_value);
                }
                ui.horizontal(|ui| {
                    save = ui.button(s.save).clicked();
                    close = ui.button(s.close).clicked();
                });
            });
            open &= !close;
            if save {
                match draft.apply(&self.settings) {
                    Some(settings) => {
                        self.apply_settings(settings, ctx);
                        open = false;
                    }
                    None => draft.error = true,
                }
            }
            if open {
                self.settings_window = Some(draft);
            }
        }
        if self.about_open {
            let mut open = true;
            egui::Window::new(s.about).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.heading("Oxycat");
                ui.label(format!("{} {}", s.version, env!("CARGO_PKG_VERSION")));
                ui.label(s.about_text);
                ui.label(format!("{} EUPL-1.2", s.license));
                ui.hyperlink(env!("CARGO_PKG_REPOSITORY"));
            });
            self.about_open = open;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SettingsDraft;
    use crate::config::Settings;

    #[test]
    fn draft_validation() {
        let base = Settings::default();
        let mut draft = SettingsDraft::from(&base);
        assert_eq!(draft.apply(&base), Some(base.clone()));
        draft.hit_limit = "0".to_owned();
        assert_eq!(draft.apply(&base), None);
        draft.hit_limit = "50".to_owned();
        draft.date_format = "%Q".to_owned();
        assert_eq!(draft.apply(&base), None);
        draft.date_format = "%Y".to_owned();
        draft.thousands_separator = " ".to_owned();
        let applied = draft.apply(&base).unwrap();
        assert_eq!((applied.hit_limit, applied.date_format.as_deref()), (50, Some("%Y")));
        assert_eq!(applied.thousands_separator.as_deref(), Some(" "));
    }
}
