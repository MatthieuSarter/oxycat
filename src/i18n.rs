//! User interface languages and translated strings.
//!
//! Each language is a [`Strings`] static. Because a static must set every field, a missing translation is a compile
//! error. Placeholders are written `{name}` and filled with [`fill`].

use serde::{Deserialize, Serialize};

use crate::core::units::NumberFormat;

/// A supported user interface language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    /// French.
    Fr,
    /// English.
    En,
    /// Spanish.
    Es,
}

impl Lang {
    /// Every supported language, in the order shown in the language selector.
    pub const ALL: [Lang; 3] = [Lang::Fr, Lang::En, Lang::Es];

    /// The language's own name, as shown in the selector.
    #[must_use]
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::Fr => "Français",
            Lang::En => "English",
            Lang::Es => "Español",
        }
    }
}

/// Picks the interface language from an OS locale such as `fr-FR` or `es_ES.UTF-8`; English when unknown.
#[must_use]
pub fn detect(locale: Option<&str>) -> Lang {
    let prefix = locale.unwrap_or_default().get(..2).unwrap_or_default().to_ascii_lowercase();
    match prefix.as_str() {
        "fr" => Lang::Fr,
        "es" => Lang::Es,
        _ => Lang::En,
    }
}

/// Number and size formatting conventions of a language.
#[must_use]
pub fn number_format(lang: Lang) -> NumberFormat {
    match lang {
        Lang::Fr => NumberFormat {
            thousands: std::borrow::Cow::Borrowed("\u{202F}"),
            decimal: ',',
            units: ["o", "Ko", "Mo", "Go", "To"],
        },
        Lang::En => NumberFormat {
            thousands: std::borrow::Cow::Borrowed(","),
            decimal: '.',
            units: ["B", "KB", "MB", "GB", "TB"],
        },
        Lang::Es => NumberFormat {
            thousands: std::borrow::Cow::Borrowed("."),
            decimal: ',',
            units: ["B", "KB", "MB", "GB", "TB"],
        },
    }
}

/// Default `strftime`-style date-time format of a language.
#[must_use]
pub fn date_format(lang: Lang) -> &'static str {
    match lang {
        Lang::Fr | Lang::Es => "%d/%m/%Y %H:%M",
        Lang::En => "%Y-%m-%d %H:%M",
    }
}

/// Replaces each `{key}` placeholder of `template` with its value.
#[must_use]
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    values.iter().fold(template.to_owned(), |text, (key, value)| text.replace(&format!("{{{key}}}"), value))
}

/// Every user-visible text of the application, in one language.
pub struct Strings {
    /// CLI: catalog created. Placeholders: `{name}`, `{files}`, `{dirs}`.
    pub cli_created: &'static str,
    /// CLI: catalog refreshed. Placeholders: `{name}`, `{files}`, `{dirs}`.
    pub cli_refreshed: &'static str,
    /// CLI: name taken by another medium. Placeholder: `{name}`.
    pub cli_name_taken: &'static str,
    /// Error: the source path does not exist. Placeholder: `{path}`.
    pub err_source_missing: &'static str,
    /// Error: preserved catalogs are never overwritten. Placeholder: `{name}`.
    pub err_preserved: &'static str,
    /// Error: scan cancelled.
    pub err_cancelled: &'static str,
    /// Error: catalog file locked by another program. Placeholder: `{name}`.
    pub err_in_use: &'static str,
    /// Error: anything else. Placeholder: `{error}`.
    pub err_other: &'static str,
    /// Invalid settings file. Placeholder: `{warning}`.
    pub settings_invalid: &'static str,
    /// Error: the catalog directory cannot be created. Placeholders: `{dir}`, `{error}`.
    pub err_catalog_dir: &'static str,

    // Top bar.
    pub theme_light: &'static str,
    pub theme_dark: &'static str,
    pub theme_auto: &'static str,
    pub settings: &'static str,
    pub about: &'static str,
    pub collapse_panel: &'static str,
    pub expand_panel: &'static str,

    // Catalog panel.
    pub catalogs: &'static str,
    pub add: &'static str,
    pub filter_hint: &'static str,
    pub check_all: &'static str,
    pub uncheck_all: &'static str,
    pub badge_archive: &'static str,
    pub no_catalogs: &'static str,
    pub invalid_file: &'static str,
    pub comment_hint: &'static str,
    pub archive: &'static str,
    pub preserved: &'static str,
    pub used: &'static str,
    pub free: &'static str,
    pub files: &'static str,
    pub folders: &'static str,
    pub skipped: &'static str,
    pub serial: &'static str,
    pub source: &'static str,
    pub cataloged_on: &'static str,
    pub refresh: &'static str,
    pub rename: &'static str,
    pub delete: &'static str,
    pub browse: &'static str,
    /// Placeholder: `{name}`.
    pub confirm_delete: &'static str,
    /// Placeholder: `{name}`.
    pub confirm_overwrite: &'static str,
    pub ok: &'static str,
    pub cancel: &'static str,
    pub new_name: &'static str,
    /// Placeholder: `{name}`.
    pub medium_not_found: &'static str,

    // Add / refresh dialog.
    pub add_title: &'static str,
    pub choose_folder: &'static str,
    pub name: &'static str,
    pub comment: &'static str,
    pub ignore: &'static str,
    pub ignore_hint: &'static str,
    pub ignore_next_refresh: &'static str,
    pub start: &'static str,
    /// Placeholder: `{name}`.
    pub scanning: &'static str,
    /// Placeholder: `{n}`.
    pub entries_found: &'static str,
    /// Placeholder: `{n}`.
    pub entries_skipped: &'static str,

    // Search tab.
    pub tab_search: &'static str,
    pub tab_folders: &'static str,
    pub search_hint: &'static str,
    pub options: &'static str,
    pub search: &'static str,
    pub date: &'static str,
    pub from: &'static str,
    pub to: &'static str,
    pub size: &'static str,
    pub min: &'static str,
    pub max: &'static str,
    pub mode: &'static str,
    pub kind: &'static str,
    pub kind_all: &'static str,
    pub kind_files: &'static str,
    pub kind_dirs: &'static str,
    pub reset: &'static str,
    pub mode_standard: &'static str,
    pub mode_duplicates: &'static str,
    pub mode_same_names: &'static str,
    pub mode_same_sizes: &'static str,
    pub mode_singles: &'static str,
    pub mode_parents: &'static str,
    pub mode_not_archived: &'static str,
    pub mode_archived: &'static str,
    pub col_name: &'static str,
    pub col_size: &'static str,
    pub col_date: &'static str,
    pub col_path: &'static str,
    pub col_catalog: &'static str,
    /// Placeholder: `{n}`.
    pub results: &'static str,
    /// Placeholder: `{n}`.
    pub catalogs_searched: &'static str,
    /// Placeholder: `{n}`, `{size}`.
    pub selection: &'static str,
    /// Placeholder: `{n}`.
    pub limit_reached: &'static str,
    pub within_results: &'static str,
    /// Placeholders: `{n}`, `{total}`.
    pub results_within: &'static str,
    pub within_truncated: &'static str,
    pub group_identical: &'static str,
    pub group_identical_off: &'static str,
    pub choose_volume: &'static str,
    pub searching: &'static str,
    pub invalid_value: &'static str,
    pub too_many_wildcards: &'static str,
    pub unbalanced_parentheses: &'static str,
    pub missing_operand: &'static str,
    pub missing_operator: &'static str,
    pub unterminated_quote: &'static str,
    /// Placeholder: `{names}`.
    pub search_errors: &'static str,
    pub double_click_hint: &'static str,
    pub no_catalog_checked: &'static str,

    // Folders tab.
    pub up: &'static str,
    pub prev_catalog: &'static str,
    pub next_catalog: &'static str,
    /// Placeholder: `{n}`, `{size}`.
    pub elements: &'static str,
    /// Placeholder: `{path}`.
    pub mounted_on: &'static str,
    pub not_mounted: &'static str,
    pub keys_hint: &'static str,
    pub choose_catalog: &'static str,

    // File actions.
    pub open: &'static str,
    pub show_in_explorer: &'static str,
    pub properties: &'static str,
    pub copy: &'static str,
    pub copy_name: &'static str,
    pub copy_full_path: &'static str,
    pub copy_parent_path: &'static str,
    pub copy_row: &'static str,
    pub export_tree: &'static str,
    pub needs_medium: &'static str,
    pub entry_missing: &'static str,

    // Settings and about windows.
    pub hit_limit: &'static str,
    pub default_ignore: &'static str,
    pub date_format: &'static str,
    pub thousands_separator: &'static str,
    pub language_default: &'static str,
    pub save: &'static str,
    pub close: &'static str,
    pub about_text: &'static str,
    pub license: &'static str,
    pub version: &'static str,
}

/// French strings.
pub static FR: Strings = Strings {
    cli_created: "Catalogue « {name} » créé : {files} fichiers, {dirs} dossiers.",
    cli_refreshed: "Catalogue « {name} » rafraîchi : {files} fichiers, {dirs} dossiers.",
    cli_name_taken: "Un catalogue « {name} » existe déjà pour un autre support ; utilisez --overwrite pour le remplacer.",
    err_source_missing: "La source « {path} » est introuvable.",
    err_preserved: "Le catalogue « {name} » est préservé et ne peut pas être écrasé.",
    err_cancelled: "Catalogage annulé ; le catalogue précédent est inchangé.",
    err_in_use: "Le catalogue « {name} » est utilisé par un autre programme ; le catalogue précédent est inchangé.",
    err_other: "Erreur : {error}",
    settings_invalid: "Réglages ignorés (fichier invalide) : {warning}",
    err_catalog_dir: "Impossible de créer le dossier des catalogues « {dir} » : {error}",
    theme_light: "☀ Clair",
    theme_dark: "🌙 Sombre",
    theme_auto: "Auto",
    settings: "Réglages",
    about: "À propos",
    collapse_panel: "Réduire le panneau",
    expand_panel: "Agrandir le panneau",
    catalogs: "Catalogues",
    add: "+ Ajouter",
    filter_hint: "🔎 Filtrer (nom, commentaire)…",
    check_all: "Tout cocher",
    uncheck_all: "Tout décocher",
    badge_archive: "Archive",
    no_catalogs: "Aucun catalogue : cliquez sur « + Ajouter » ou déposez un disque ici.",
    invalid_file: "Fichier invalide",
    comment_hint: "Commentaire (Entrée pour valider)",
    archive: "Archive",
    preserved: "Préservé",
    used: "Utilisé",
    free: "Libre",
    files: "Fichiers",
    folders: "Dossiers",
    skipped: "Illisibles",
    serial: "N° de série",
    source: "Source",
    cataloged_on: "Catalogué le",
    refresh: "Rafraîchir",
    rename: "Renommer",
    delete: "Supprimer",
    browse: "Parcourir",
    confirm_delete: "Supprimer définitivement le catalogue « {name} » ?",
    confirm_overwrite: "Un catalogue « {name} » existe déjà pour un autre support. Le remplacer ?",
    ok: "OK",
    cancel: "Annuler",
    new_name: "Nouveau nom",
    medium_not_found: "Le support du catalogue « {name} » est introuvable : branchez-le, ou utilisez « + Ajouter ».",
    add_title: "Cataloguer un support",
    choose_folder: "Choisir…",
    name: "Nom",
    comment: "Commentaire",
    ignore: "Ignorer",
    ignore_hint: "*.tmp; \\Temp*",
    ignore_next_refresh: "Appliqué au prochain rafraîchissement",
    start: "Cataloguer",
    scanning: "Catalogage de « {name} »…",
    entries_found: "{n} éléments trouvés",
    entries_skipped: "{n} illisibles",
    tab_search: "🔎 Recherche",
    tab_folders: "Dossiers",
    search_hint: "Partie du nom ; | marque le début ou la fin, un seul *",
    options: "Options",
    search: "Rechercher",
    date: "Date",
    from: "du",
    to: "au",
    size: "Taille",
    min: "≥",
    max: "≤",
    mode: "Mode",
    kind: "Type",
    kind_all: "Tout",
    kind_files: "Fichiers",
    kind_dirs: "Dossiers",
    reset: "Réinitialiser",
    mode_standard: "Standard",
    mode_duplicates: "Doublons (nom et taille)",
    mode_same_names: "Mêmes noms",
    mode_same_sizes: "Mêmes tailles",
    mode_singles: "Uniques",
    mode_parents: "Dossier parent",
    mode_not_archived: "Non archivés",
    mode_archived: "Archivés",
    col_name: "Nom",
    col_size: "Taille",
    col_date: "Date",
    col_path: "Chemin",
    col_catalog: "Volume",
    results: "{n} résultats",
    catalogs_searched: "{n} catalogues",
    selection: "{n} sélectionnés · {size}",
    limit_reached: "Limite de {n} résultats atteinte : affinez la recherche",
    within_results: "Rechercher dans les résultats",
    results_within: "{n} résultats parmi {total}",
    within_truncated: "Recherche faite dans des résultats tronqués : des correspondances peuvent manquer",
    group_identical: "Regrouper les résultats identiques",
    group_identical_off: "Sans effet dans ce mode, qui montre justement les copies",
    choose_volume: "Sur quel volume ?",
    searching: "Recherche…",
    invalid_value: "Valeur invalide",
    too_many_wildcards: "Un seul * est autorisé",
    unbalanced_parentheses: "Parenthèses non appariées",
    missing_operand: "Texte manquant autour de AND / OR ou entre parenthèses",
    missing_operator: "AND ou OR manquant entre deux termes",
    unterminated_quote: "Guillemet non fermé",
    search_errors: "Catalogues illisibles : {names}",
    double_click_hint: "Double-clic : ouvrir le dossier",
    no_catalog_checked: "Cochez au moins un catalogue.",
    up: "⬆",
    prev_catalog: "Catalogue précédent (Ctrl+P)",
    next_catalog: "Catalogue suivant (Ctrl+N)",
    elements: "{n} éléments · {size}",
    mounted_on: "⚫ Support branché sur {path}",
    not_mounted: "○ Support non branché",
    keys_hint: "Ctrl+P / Ctrl+N · Retour arrière",
    choose_catalog: "Double-cliquez sur un catalogue ou un résultat de recherche.",
    open: "Ouvrir",
    show_in_explorer: "Afficher dans l'Explorateur",
    properties: "Propriétés",
    copy: "Copier",
    copy_name: "Nom (Ctrl+Maj+C)",
    copy_full_path: "Chemin complet (Ctrl+C)",
    copy_parent_path: "Chemin parent",
    copy_row: "Ligne complète",
    export_tree: "Exporter l'arborescence (.txt)…",
    needs_medium: "Branchez le support pour cette action",
    entry_missing: "Introuvable sur le support branché : {path}",
    hit_limit: "Nombre maximum de résultats",
    default_ignore: "Motifs ignorés par défaut",
    date_format: "Format des dates (vide = selon la langue)",
    thousands_separator: "Séparateur de milliers (vide = selon la langue)",
    language_default: "Langue du système",
    save: "Enregistrer",
    close: "Fermer",
    about_text: "Catalogue vos supports de stockage pour y chercher même débranchés.",
    license: "Licence",
    version: "Version",
};

/// English strings.
pub static EN: Strings = Strings {
    cli_created: "Catalog \"{name}\" created: {files} files, {dirs} folders.",
    cli_refreshed: "Catalog \"{name}\" refreshed: {files} files, {dirs} folders.",
    cli_name_taken: "A catalog \"{name}\" already exists for another medium; use --overwrite to replace it.",
    err_source_missing: "Source \"{path}\" not found.",
    err_preserved: "Catalog \"{name}\" is preserved and cannot be overwritten.",
    err_cancelled: "Cataloging cancelled; the previous catalog is unchanged.",
    err_in_use: "Catalog \"{name}\" is in use by another program; the previous catalog is unchanged.",
    err_other: "Error: {error}",
    settings_invalid: "Settings ignored (invalid file): {warning}",
    err_catalog_dir: "Cannot create the catalog directory \"{dir}\": {error}",
    theme_light: "☀ Light",
    theme_dark: "🌙 Dark",
    theme_auto: "Auto",
    settings: "Settings",
    about: "About",
    collapse_panel: "Collapse the panel",
    expand_panel: "Expand the panel",
    catalogs: "Catalogs",
    add: "+ Add",
    filter_hint: "🔎 Filter (name, comment)…",
    check_all: "Check all",
    uncheck_all: "Uncheck all",
    badge_archive: "Archive",
    no_catalogs: "No catalog yet: click \"+ Add\" or drop a drive here.",
    invalid_file: "Invalid file",
    comment_hint: "Comment (Enter to save)",
    archive: "Archive",
    preserved: "Preserved",
    used: "Used",
    free: "Free",
    files: "Files",
    folders: "Folders",
    skipped: "Unreadable",
    serial: "Serial number",
    source: "Source",
    cataloged_on: "Cataloged on",
    refresh: "Refresh",
    rename: "Rename",
    delete: "Delete",
    browse: "Browse",
    confirm_delete: "Permanently delete the catalog \"{name}\"?",
    confirm_overwrite: "A catalog \"{name}\" already exists for another medium. Replace it?",
    ok: "OK",
    cancel: "Cancel",
    new_name: "New name",
    medium_not_found: "The medium of catalog \"{name}\" cannot be found: plug it in, or use \"+ Add\".",
    add_title: "Catalog a medium",
    choose_folder: "Choose…",
    name: "Name",
    comment: "Comment",
    ignore: "Ignore",
    ignore_hint: "*.tmp; \\Temp*",
    ignore_next_refresh: "Applied at the next refresh",
    start: "Catalog",
    scanning: "Cataloging \"{name}\"…",
    entries_found: "{n} entries found",
    entries_skipped: "{n} unreadable",
    tab_search: "🔎 Search",
    tab_folders: "Folders",
    search_hint: "Part of the name; | marks start or end, a single *",
    options: "Options",
    search: "Search",
    date: "Date",
    from: "from",
    to: "to",
    size: "Size",
    min: "≥",
    max: "≤",
    mode: "Mode",
    kind: "Type",
    kind_all: "All",
    kind_files: "Files",
    kind_dirs: "Folders",
    reset: "Reset",
    mode_standard: "Standard",
    mode_duplicates: "Duplicates (name and size)",
    mode_same_names: "Same names",
    mode_same_sizes: "Same sizes",
    mode_singles: "Singles",
    mode_parents: "Parent folder",
    mode_not_archived: "Not archived",
    mode_archived: "Archived",
    col_name: "Name",
    col_size: "Size",
    col_date: "Date",
    col_path: "Path",
    col_catalog: "Volume",
    results: "{n} results",
    catalogs_searched: "{n} catalogs",
    selection: "{n} selected · {size}",
    limit_reached: "Limit of {n} results reached: refine the search",
    within_results: "Search within results",
    results_within: "{n} results within {total}",
    within_truncated: "Searched within truncated results: some matches may be missing",
    group_identical: "Group identical results",
    group_identical_off: "Not available in this mode, which shows the copies",
    choose_volume: "Which volume?",
    searching: "Searching…",
    invalid_value: "Invalid value",
    too_many_wildcards: "Only one * is allowed",
    unbalanced_parentheses: "Unbalanced parentheses",
    missing_operand: "Missing text around AND / OR or inside parentheses",
    missing_operator: "Missing AND or OR between two terms",
    unterminated_quote: "Unclosed quote",
    search_errors: "Unreadable catalogs: {names}",
    double_click_hint: "Double-click: open the folder",
    no_catalog_checked: "Check at least one catalog.",
    up: "⬆",
    prev_catalog: "Previous catalog (Ctrl+P)",
    next_catalog: "Next catalog (Ctrl+N)",
    elements: "{n} items · {size}",
    mounted_on: "⚫ Medium mounted on {path}",
    not_mounted: "○ Medium not mounted",
    keys_hint: "Ctrl+P / Ctrl+N · Backspace",
    choose_catalog: "Double-click a catalog or a search result.",
    open: "Open",
    show_in_explorer: "Show in file manager",
    properties: "Properties",
    copy: "Copy",
    copy_name: "Name (Ctrl+Shift+C)",
    copy_full_path: "Full path (Ctrl+C)",
    copy_parent_path: "Parent path",
    copy_row: "Whole row",
    export_tree: "Export folder tree (.txt)…",
    needs_medium: "Plug the medium in for this action",
    entry_missing: "Not found on the plugged-in medium: {path}",
    hit_limit: "Maximum number of results",
    default_ignore: "Default ignore patterns",
    date_format: "Date format (empty = language default)",
    thousands_separator: "Thousands separator (empty = language default)",
    language_default: "System language",
    save: "Save",
    close: "Close",
    about_text: "Catalogs your storage media so you can search them even when unplugged.",
    license: "License",
    version: "Version",
};

/// Spanish strings.
pub static ES: Strings = Strings {
    cli_created: "Catálogo «{name}» creado: {files} archivos, {dirs} carpetas.",
    cli_refreshed: "Catálogo «{name}» actualizado: {files} archivos, {dirs} carpetas.",
    cli_name_taken: "Ya existe un catálogo «{name}» para otro soporte; use --overwrite para reemplazarlo.",
    err_source_missing: "No se encuentra el origen «{path}».",
    err_preserved: "El catálogo «{name}» está protegido y no se puede sobrescribir.",
    err_cancelled: "Catalogación cancelada; el catálogo anterior no ha cambiado.",
    err_in_use: "El catálogo «{name}» está siendo usado por otro programa; el catálogo anterior no ha cambiado.",
    err_other: "Error: {error}",
    settings_invalid: "Ajustes ignorados (archivo no válido): {warning}",
    err_catalog_dir: "No se puede crear la carpeta de catálogos «{dir}»: {error}",
    theme_light: "☀ Claro",
    theme_dark: "🌙 Oscuro",
    theme_auto: "Auto",
    settings: "Ajustes",
    about: "Acerca de",
    collapse_panel: "Contraer el panel",
    expand_panel: "Expandir el panel",
    catalogs: "Catálogos",
    add: "+ Añadir",
    filter_hint: "🔎 Filtrar (nombre, comentario)…",
    check_all: "Marcar todos",
    uncheck_all: "Desmarcar todos",
    badge_archive: "Archivo",
    no_catalogs: "Ningún catálogo: pulse «+ Añadir» o suelte un disco aquí.",
    invalid_file: "Archivo no válido",
    comment_hint: "Comentario (Intro para guardar)",
    archive: "Archivo",
    preserved: "Protegido",
    used: "Usado",
    free: "Libre",
    files: "Archivos",
    folders: "Carpetas",
    skipped: "Ilegibles",
    serial: "N.º de serie",
    source: "Origen",
    cataloged_on: "Catalogado el",
    refresh: "Actualizar",
    rename: "Renombrar",
    delete: "Eliminar",
    browse: "Explorar",
    confirm_delete: "¿Eliminar definitivamente el catálogo «{name}»?",
    confirm_overwrite: "Ya existe un catálogo «{name}» para otro soporte. ¿Reemplazarlo?",
    ok: "Aceptar",
    cancel: "Cancelar",
    new_name: "Nuevo nombre",
    medium_not_found: "No se encuentra el soporte del catálogo «{name}»: conéctelo o use «+ Añadir».",
    add_title: "Catalogar un soporte",
    choose_folder: "Elegir…",
    name: "Nombre",
    comment: "Comentario",
    ignore: "Ignorar",
    ignore_hint: "*.tmp; \\Temp*",
    ignore_next_refresh: "Se aplica en la próxima actualización",
    start: "Catalogar",
    scanning: "Catalogando «{name}»…",
    entries_found: "{n} elementos encontrados",
    entries_skipped: "{n} ilegibles",
    tab_search: "🔎 Búsqueda",
    tab_folders: "Carpetas",
    search_hint: "Parte del nombre; | marca inicio o fin, un solo *",
    options: "Opciones",
    search: "Buscar",
    date: "Fecha",
    from: "desde",
    to: "hasta",
    size: "Tamaño",
    min: "≥",
    max: "≤",
    mode: "Modo",
    kind: "Tipo",
    kind_all: "Todo",
    kind_files: "Archivos",
    kind_dirs: "Carpetas",
    reset: "Restablecer",
    mode_standard: "Estándar",
    mode_duplicates: "Duplicados (nombre y tamaño)",
    mode_same_names: "Mismos nombres",
    mode_same_sizes: "Mismos tamaños",
    mode_singles: "Únicos",
    mode_parents: "Carpeta padre",
    mode_not_archived: "No archivados",
    mode_archived: "Archivados",
    col_name: "Nombre",
    col_size: "Tamaño",
    col_date: "Fecha",
    col_path: "Ruta",
    col_catalog: "Volumen",
    results: "{n} resultados",
    catalogs_searched: "{n} catálogos",
    selection: "{n} seleccionados · {size}",
    limit_reached: "Límite de {n} resultados alcanzado: afine la búsqueda",
    within_results: "Buscar en los resultados",
    results_within: "{n} resultados de {total}",
    within_truncated: "Búsqueda en resultados truncados: pueden faltar coincidencias",
    group_identical: "Agrupar resultados idénticos",
    group_identical_off: "No disponible en este modo, que muestra las copias",
    choose_volume: "¿En qué volumen?",
    searching: "Buscando…",
    invalid_value: "Valor no válido",
    too_many_wildcards: "Solo se admite un *",
    unbalanced_parentheses: "Paréntesis desequilibrados",
    missing_operand: "Falta texto alrededor de AND / OR o entre paréntesis",
    missing_operator: "Falta AND u OR entre dos términos",
    unterminated_quote: "Comillas sin cerrar",
    search_errors: "Catálogos ilegibles: {names}",
    double_click_hint: "Doble clic: abrir la carpeta",
    no_catalog_checked: "Marque al menos un catálogo.",
    up: "⬆",
    prev_catalog: "Catálogo anterior (Ctrl+P)",
    next_catalog: "Catálogo siguiente (Ctrl+N)",
    elements: "{n} elementos · {size}",
    mounted_on: "⚫ Soporte conectado en {path}",
    not_mounted: "○ Soporte no conectado",
    keys_hint: "Ctrl+P / Ctrl+N · Retroceso",
    choose_catalog: "Haga doble clic en un catálogo o en un resultado de búsqueda.",
    open: "Abrir",
    show_in_explorer: "Mostrar en el explorador",
    properties: "Propiedades",
    copy: "Copiar",
    copy_name: "Nombre (Ctrl+Mayús+C)",
    copy_full_path: "Ruta completa (Ctrl+C)",
    copy_parent_path: "Ruta padre",
    copy_row: "Fila completa",
    export_tree: "Exportar el árbol de carpetas (.txt)…",
    needs_medium: "Conecte el soporte para esta acción",
    entry_missing: "No encontrado en el soporte conectado: {path}",
    hit_limit: "Número máximo de resultados",
    default_ignore: "Patrones ignorados por defecto",
    date_format: "Formato de fecha (vacío = según el idioma)",
    thousands_separator: "Separador de miles (vacío = según el idioma)",
    language_default: "Idioma del sistema",
    save: "Guardar",
    close: "Cerrar",
    about_text: "Cataloga sus soportes de almacenamiento para buscar en ellos incluso desconectados.",
    license: "Licencia",
    version: "Versión",
};

/// The strings of a language.
#[must_use]
pub fn strings(lang: Lang) -> &'static Strings {
    match lang {
        Lang::Fr => &FR,
        Lang::En => &EN,
        Lang::Es => &ES,
    }
}

#[cfg(test)]
mod tests {
    use super::{Lang, date_format, detect, fill, number_format};

    #[test]
    fn detection() {
        assert_eq!(detect(Some("fr-FR")), Lang::Fr);
        assert_eq!(detect(Some("FR")), Lang::Fr);
        assert_eq!(detect(Some("es_ES.UTF-8")), Lang::Es);
        assert_eq!(detect(Some("de-DE")), Lang::En);
        assert_eq!(detect(Some("")), Lang::En);
        assert_eq!(detect(None), Lang::En);
    }

    #[test]
    fn formats_differ() {
        assert_eq!(number_format(Lang::Fr).units[2], "Mo");
        assert_eq!(number_format(Lang::Es).thousands, ".");
        assert_eq!(number_format(Lang::Es).decimal, ',');
        assert_eq!(number_format(Lang::En).decimal, '.');
        assert_eq!(date_format(Lang::Fr), "%d/%m/%Y %H:%M");
        assert_eq!(date_format(Lang::En), "%Y-%m-%d %H:%M");
    }

    #[test]
    fn fill_placeholders() {
        assert_eq!(fill("{a} and {b} and {a}", &[("a", "x"), ("b", "y")]), "x and y and x");
        assert_eq!(fill("no {c}", &[("a", "x")]), "no {c}");
    }
}
