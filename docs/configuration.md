# Configuration

## Catalog directory

Oxycat keeps every catalog (`<name>.db`) and its settings file (`oxycat.toml`) in a single **catalog directory**,
chosen in this order:

1. the `--catalogs <dir>` command-line option (GUI and CLI alike);
2. otherwise the `OXYCAT_CATALOGS` environment variable, when set and not empty;
3. otherwise the directory containing the Oxycat executable.

Nothing is written to the registry or to the user profile: copying the directory copies everything.

The directory also receives `oxycat-crash.log` when Oxycat crashes: one line per crash (UTC time, version, thread and
panic message), since the Windows executable has no console to print it to. Include it when reporting a bug.

**Back it up.** A catalog can be rebuilt by cataloging the medium again, but not once the medium is gone — which is
precisely the case of archived and preserved catalogs. Include the catalog directory in your regular backups.

## `oxycat.toml`

Every key is optional; a missing file means all defaults. An invalid file is reported in the interface and the
defaults are used (the file is not overwritten until a setting is changed).

| Key | Default | Meaning |
|---|---|---|
| `theme` | `"auto"` | `"light"`, `"dark"` or `"auto"` (follow the operating system) |
| `language` | *(OS locale)* | `"fr"`, `"en"` or `"es"`; when absent, French or Spanish OS locales select those languages, anything else English |
| `hit_limit` | `10000` | maximum number of search results |
| `default_ignore` | `""` | ignore patterns proposed when cataloging, e.g. `"*.tmp; \\Temp*"` (see the user guide) |
| `date_format` | *(language default)* | [`strftime`-style format](https://docs.rs/jiff/latest/jiff/fmt/strtime/index.html), e.g. `"%Y-%m-%d %H:%M"` |
| `thousands_separator` | *(language default)* | separator used between digit groups, e.g. `" "` |
| `catalogs_collapsed` | `false` | catalog panel collapsed to the bare list |
| `group_identical` | `true` | search results identical but for their volume shown as one row |

Language defaults: French and Spanish show dates as `%d/%m/%Y %H:%M`, English as `%Y-%m-%d %H:%M`; thousands are
separated by a narrow space in French, `,` in English and `.` in Spanish.
