//! Cathy-style ignore patterns: `*.tmp; *.jbf; \Temp*`.
//!
//! Rules are `;`-separated globs (`*` and `?`, case-insensitive). A leading `\` makes a rule apply to folder names,
//! otherwise it applies to file names.

/// A parsed set of ignore rules.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IgnoreRules {
    files: Vec<String>,
    dirs: Vec<String>,
}

impl IgnoreRules {
    /// Parses a `;`-separated list of rules; blank entries are dropped.
    #[must_use]
    pub fn parse(spec: &str) -> IgnoreRules {
        let mut rules = IgnoreRules::default();
        for rule in spec.split(';').map(str::trim).filter(|r| !r.is_empty()) {
            match rule.strip_prefix('\\') {
                Some(dir) if !dir.is_empty() => rules.dirs.push(dir.to_lowercase()),
                Some(_) => {}
                None => rules.files.push(rule.to_lowercase()),
            }
        }
        rules
    }

    /// Tells whether an entry with this name must be left out of the catalog.
    #[must_use]
    pub fn is_ignored(&self, name: &str, is_dir: bool) -> bool {
        let globs = if is_dir { &self.dirs } else { &self.files };
        if globs.is_empty() {
            return false;
        }
        let name: Vec<char> = name.to_lowercase().chars().collect();
        globs.iter().any(|g| glob_match(&g.chars().collect::<Vec<_>>(), &name))
    }
}

/// Matches `name` against a glob made of literal characters, `*` (any run) and `?` (any one character).
fn glob_match(glob: &[char], name: &[char]) -> bool {
    let (mut g, mut n) = (0, 0);
    // Position of the last `*` in the glob and the name position it currently absorbs up to.
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        if g < glob.len() && (glob[g] == '?' || glob[g] == name[n]) {
            g += 1;
            n += 1;
        } else if g < glob.len() && glob[g] == '*' {
            star = Some((g, n));
            g += 1;
        } else if let Some((sg, sn)) = star {
            g = sg + 1;
            n = sn + 1;
            star = Some((sg, sn + 1));
        } else {
            return false;
        }
    }
    glob[g..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::IgnoreRules;

    #[test]
    fn cathy_example() {
        let r = IgnoreRules::parse("*.tmp; *.jbf; \\Temp*");
        assert!(r.is_ignored("a.TMP", false));
        assert!(r.is_ignored("b.jbf", false));
        assert!(!r.is_ignored("x.tmp", true));
        assert!(r.is_ignored("Temporary Internet Files", true));
        assert!(!r.is_ignored("Temp.txt", false));
        assert!(!r.is_ignored("MyTemp", true));
    }

    #[test]
    fn question_mark() {
        let r = IgnoreRules::parse("?.log");
        assert!(r.is_ignored("a.log", false));
        assert!(!r.is_ignored("ab.log", false));
    }

    #[test]
    fn exact_name_and_unicode() {
        let r = IgnoreRules::parse("Thumbs.db;\\Été*");
        assert!(r.is_ignored("thumbs.db", false));
        assert!(!r.is_ignored("thumbs.db.bak", false));
        assert!(r.is_ignored("été 2020", true));
    }

    #[test]
    fn empty_spec_ignores_nothing() {
        assert!(!IgnoreRules::parse(" ; ").is_ignored("x", false));
        assert!(!IgnoreRules::parse("").is_ignored("x", true));
    }
}
