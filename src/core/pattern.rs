//! Cathy-style search patterns: case- and accent-insensitive substring, `|` anchors, at most one inner `*`,
//! combined into expressions with `AND`, `OR`, parentheses and double quotes.

use super::fold::fold;

/// A parsed search pattern. Parts are folded; a single empty part matches everything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    /// The name must start with the first part.
    pub anchored_start: bool,
    /// The name must end with the last part.
    pub anchored_end: bool,
    /// One part, or two parts separated by the single allowed wildcard.
    pub parts: Vec<String>,
}

/// Why a pattern was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternError {
    /// More than one `*` inside the pattern.
    TooManyWildcards,
    /// A `(` without its `)`, or the reverse.
    UnbalancedParentheses,
    /// An operator or parentheses with no text on one side: `a OR`, `AND b`, `()`.
    MissingOperand,
    /// Two operands with no operator between them: `(a) b`.
    MissingOperator,
    /// A `"` without its closing `"`.
    UnterminatedQuote,
}

impl Pattern {
    /// Parses user input.
    ///
    /// Leading `|` anchors the start, trailing `|` the end; `*` at either end is ignored (`*str` ≡ `str*` ≡ `str`);
    /// a pattern with no text matches every name.
    ///
    /// # Errors
    /// [`PatternError::TooManyWildcards`] when the text contains more than one inner `*`.
    pub fn parse(input: &str) -> Result<Pattern, PatternError> {
        let mut text = input.trim();
        let anchored_start = text.starts_with('|');
        if anchored_start {
            text = &text[1..];
        }
        let anchored_end = text.ends_with('|');
        if anchored_end {
            text = &text[..text.len() - 1];
        }
        let parts: Vec<String> = text.trim_matches('*').split('*').map(fold).collect();
        if parts.len() > 2 {
            return Err(PatternError::TooManyWildcards);
        }
        if parts.iter().all(String::is_empty) {
            return Ok(Pattern { anchored_start: false, anchored_end: false, parts: vec![String::new()] });
        }
        Ok(Pattern { anchored_start, anchored_end, parts })
    }

    /// Tells whether the pattern matches every name.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parts.iter().all(String::is_empty)
    }

    /// Tells whether an already folded name matches.
    #[must_use]
    pub fn matches(&self, folded_name: &str) -> bool {
        let (first, second) = match self.parts.as_slice() {
            [only] => (only.as_str(), None),
            [a, b] => (a.as_str(), Some(b.as_str())),
            _ => return false,
        };
        let Some(second) = second else {
            return match (self.anchored_start, self.anchored_end) {
                (true, true) => folded_name == first,
                (true, false) => folded_name.starts_with(first),
                (false, true) => folded_name.ends_with(first),
                (false, false) => folded_name.contains(first),
            };
        };
        // The leftmost occurrence of the first part leaves the most room for the second one.
        let rest = if self.anchored_start {
            folded_name.strip_prefix(first)
        } else {
            folded_name.find(first).map(|i| &folded_name[i + first.len()..])
        };
        rest.is_some_and(|rest| if self.anchored_end { rest.ends_with(second) } else { rest.contains(second) })
    }

    /// Converts the pattern to a SQL `LIKE` expression, to be used with `ESCAPE '\'` on a folded column.
    #[must_use]
    pub fn to_like(&self) -> String {
        if self.is_empty() {
            return "%".to_owned();
        }
        let body = self.parts.iter().map(|p| escape_like(p)).collect::<Vec<_>>().join("%");
        let start = if self.anchored_start { "" } else { "%" };
        let end = if self.anchored_end { "" } else { "%" };
        format!("{start}{body}{end}")
    }
}

/// A search expression: patterns combined with `AND` (binding tighter) and `OR`, grouped with parentheses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// A single pattern.
    Leaf(Pattern),
    /// Every operand must match (at least two).
    And(Vec<Expr>),
    /// At least one operand must match (at least two).
    Or(Vec<Expr>),
}

/// A lexical unit of an expression.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Open,
    Close,
    And,
    Or,
    /// Operand text, quotes removed.
    Text(String),
}

/// Tokens being parsed.
type Tokens = std::iter::Peekable<std::vec::IntoIter<Token>>;

impl Expr {
    /// Parses user input.
    ///
    /// Without an uppercase `AND` or `OR` word (outside double quotes), the whole input is a single [`Pattern`],
    /// parentheses and quotes included, exactly as before expressions existed. Otherwise `(` and `)` group, `"…"`
    /// keeps its content literal (parentheses, `AND`, `OR`), and each operand is a [`Pattern`].
    ///
    /// # Errors
    /// The [`PatternError`] of a malformed expression or of an invalid operand.
    pub fn parse(input: &str) -> Result<Expr, PatternError> {
        let mut tokens = Vec::new();
        let lexed = lex(input, &mut tokens);
        if !tokens.iter().any(|t| matches!(t, Token::And | Token::Or)) {
            return Pattern::parse(input).map(Expr::Leaf);
        }
        lexed?;
        let mut tokens = tokens.into_iter().peekable();
        let expr = parse_or(&mut tokens)?;
        match tokens.next() {
            None => Ok(expr),
            Some(Token::Close) => Err(PatternError::UnbalancedParentheses),
            Some(_) => Err(PatternError::MissingOperator),
        }
    }

    /// Tells whether the expression matches every name.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        matches!(self, Expr::Leaf(p) if p.is_empty())
    }

    /// Tells whether an already folded name matches.
    #[must_use]
    pub fn matches(&self, folded_name: &str) -> bool {
        match self {
            Expr::Leaf(p) => p.matches(folded_name),
            Expr::And(operands) => operands.iter().all(|e| e.matches(folded_name)),
            Expr::Or(operands) => operands.iter().any(|e| e.matches(folded_name)),
        }
    }

    /// Converts the expression to a SQL condition on a folded `column`, with one `?` per pattern; the `LIKE` values
    /// are appended to `likes` in placeholder order.
    #[must_use]
    pub fn to_sql(&self, column: &str, likes: &mut Vec<String>) -> String {
        let (operands, operator) = match self {
            Expr::Leaf(p) => {
                likes.push(p.to_like());
                return format!("{column} LIKE ? ESCAPE '\\'");
            }
            Expr::And(operands) => (operands, " AND "),
            Expr::Or(operands) => (operands, " OR "),
        };
        let parts: Vec<String> = operands.iter().map(|e| e.to_sql(column, likes)).collect();
        format!("({})", parts.join(operator))
    }
}

/// Splits `input` into `tokens`; on an unclosed quote, `tokens` keeps what was read before it.
fn lex(input: &str, tokens: &mut Vec<Token>) -> Result<(), PatternError> {
    let mut text = String::new();
    let mut quoted = false;
    let mut prev: Option<char> = None;
    let mut i = 0;
    while let Some(c) = input[i..].chars().next() {
        let operator = if prev.is_none_or(is_boundary) { operator_at(&input[i..]) } else { None };
        if let Some((token, word)) = operator {
            flush(&mut text, &mut quoted, tokens);
            tokens.push(token);
            i += word.len();
            prev = word.chars().next_back();
            continue;
        }
        match c {
            '(' | ')' => {
                flush(&mut text, &mut quoted, tokens);
                tokens.push(if c == '(' { Token::Open } else { Token::Close });
            }
            '"' => {
                let content = &input[i + 1..];
                let end = content.find('"').ok_or(PatternError::UnterminatedQuote)?;
                text.push_str(&content[..end]);
                quoted = true;
                // Skips the content and the opening quote; the closing one is skipped below, as the previous char.
                i += end + 1;
            }
            _ => text.push(c),
        }
        prev = Some(c);
        i += c.len_utf8();
    }
    flush(&mut text, &mut quoted, tokens);
    Ok(())
}

/// The operator starting `rest` when it is a whole word, with its spelling.
fn operator_at(rest: &str) -> Option<(Token, &'static str)> {
    [(Token::And, "AND"), (Token::Or, "OR")]
        .into_iter()
        .find(|(_, word)| rest.strip_prefix(word).is_some_and(|after| after.chars().next().is_none_or(is_boundary)))
}

/// Characters that can surround an operator word.
fn is_boundary(c: char) -> bool {
    c.is_whitespace() || matches!(c, '(' | ')' | '"')
}

/// Ends the current operand text; blank unquoted text (spaces around operators) is dropped.
fn flush(text: &mut String, quoted: &mut bool, tokens: &mut Vec<Token>) {
    if *quoted || !text.trim().is_empty() {
        tokens.push(Token::Text(std::mem::take(text)));
    }
    text.clear();
    *quoted = false;
}

/// `or := and (OR and)*`
fn parse_or(tokens: &mut Tokens) -> Result<Expr, PatternError> {
    let mut operands = vec![parse_and(tokens)?];
    while tokens.next_if_eq(&Token::Or).is_some() {
        operands.push(parse_and(tokens)?);
    }
    Ok(combine(operands, Expr::Or))
}

/// `and := operand (AND operand)*`
fn parse_and(tokens: &mut Tokens) -> Result<Expr, PatternError> {
    let mut operands = vec![parse_operand(tokens)?];
    while tokens.next_if_eq(&Token::And).is_some() {
        operands.push(parse_operand(tokens)?);
    }
    Ok(combine(operands, Expr::And))
}

/// `operand := "(" or ")" | text`
fn parse_operand(tokens: &mut Tokens) -> Result<Expr, PatternError> {
    match tokens.next() {
        Some(Token::Open) => {
            let inner = parse_or(tokens)?;
            match tokens.next() {
                Some(Token::Close) => Ok(inner),
                None => Err(PatternError::UnbalancedParentheses),
                Some(_) => Err(PatternError::MissingOperator),
            }
        }
        Some(Token::Text(text)) if !text.trim().is_empty() => Pattern::parse(&text).map(Expr::Leaf),
        _ => Err(PatternError::MissingOperand),
    }
}

/// A lone operand as is, several ones under `node`.
fn combine(mut operands: Vec<Expr>, node: fn(Vec<Expr>) -> Expr) -> Expr {
    if operands.len() == 1 { operands.remove(0) } else { node(operands) }
}

/// Escapes the `LIKE` metacharacters `\`, `%` and `_` with a backslash.
fn escape_like(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Expr, Pattern, PatternError};
    use crate::core::fold::fold;

    fn p(s: &str) -> Pattern {
        Pattern::parse(s).unwrap()
    }

    #[test]
    fn ext_anchor() {
        assert!(p(".c|").matches("main.c"));
        assert!(!p(".c|").matches("main.cpp"));
    }

    #[test]
    fn both_anchors_wildcard() {
        let pat = p("|s*doc|");
        assert!(pat.matches("solution.doc"));
        assert!(pat.matches("stringdoc"));
        assert!(!pat.matches("soldocument.txt"));
    }

    #[test]
    fn start_anchor_only() {
        let pat = p("|s*doc");
        assert!(pat.matches("soldocument.txt"));
        assert!(!pat.matches("unsoldocument.txt"));
    }

    #[test]
    fn unanchored_wildcard() {
        assert!(p("s*doc").matches("unsoldocument.txt"));
        assert!(!p("s*doc").matches("document"));
    }

    #[test]
    fn wildcard_parts_do_not_overlap() {
        assert!(!p("|ab*bc|").matches("abc"));
        assert!(p("|ab*bc|").matches("abbc"));
    }

    #[test]
    fn edge_stars_equivalent() {
        for s in ["*str", "str*", "str"] {
            assert_eq!(p(s), p("str"));
        }
    }

    #[test]
    fn accents_folded() {
        assert!(p("ASTERIX").matches(&fold("Astérix 01.cbz")));
        assert!(p("astérix").matches("asterix"));
    }

    #[test]
    fn two_inner_stars_rejected() {
        assert_eq!(Pattern::parse("a*b*c"), Err(PatternError::TooManyWildcards));
    }

    #[test]
    fn empty_and_bars_match_all() {
        for s in ["", "   ", "|", "||", "*"] {
            assert!(p(s).matches("anything"), "{s:?}");
        }
    }

    #[test]
    fn like_escapes_metachars() {
        assert_eq!(p("100%").to_like(), r"%100\%%");
        assert_eq!(p("|my_file").to_like(), r"my\_file%");
        assert_eq!(p("|s*doc|").to_like(), "s%doc");
        assert_eq!(p(r"a\b").to_like(), r"%a\\b%");
        assert_eq!(p("").to_like(), "%");
    }

    fn e(s: &str) -> Expr {
        Expr::parse(s).unwrap()
    }

    #[test]
    fn expr_without_operator_is_a_plain_pattern() {
        for s in ["Movie (2019)", "rock and roll", "a \"b", "(", "ANDROID", "\"x OR y", "|s*doc|", ""] {
            assert_eq!(Expr::parse(s), Pattern::parse(s).map(Expr::Leaf), "{s:?}");
        }
        assert_eq!(Expr::parse("a*b*c"), Err(PatternError::TooManyWildcards));
    }

    #[test]
    fn expr_or_and() {
        assert!(e("asterix OR obelix").matches("obelix 02"));
        assert!(!e("asterix AND obelix").matches("obelix 02"));
        assert!(e("asterix AND obelix").matches("asterix et obelix"));
    }

    #[test]
    fn expr_and_binds_tighter() {
        assert_eq!(e("a OR b AND c"), Expr::Or(vec![e("a"), Expr::And(vec![e("b"), e("c")])]));
        assert!(e("a OR b AND c").matches("a"));
        assert!(!e("(a OR b) AND c").matches("a"));
        assert!(e("(a OR b) AND c").matches("bc"));
    }

    #[test]
    fn expr_operands_keep_pattern_syntax() {
        let pat = e("(asterix OR obelix) AND .cbz|");
        assert!(pat.matches(&fold("Astérix 01.cbz")));
        assert!(!pat.matches("asterix.cbz.txt"));
        assert!(e("|s*doc| OR x").matches("solution.doc"));
        assert_eq!(Expr::parse("a*b*c OR d"), Err(PatternError::TooManyWildcards));
    }

    #[test]
    fn expr_operator_words_need_boundaries() {
        assert_eq!(e("SAND OR x"), Expr::Or(vec![e("SAND"), e("x")]));
        assert_eq!(e("a AND(b)"), Expr::And(vec![e("a"), e("b")]));
        assert_eq!(e("ANDROID OR ORANGE"), Expr::Or(vec![e("ANDROID"), e("ORANGE")]));
        assert_eq!(e("rock and roll OR x"), Expr::Or(vec![e("rock and roll"), e("x")]));
    }

    #[test]
    fn expr_quotes_are_literal() {
        assert_eq!(
            e("\"Movie (2019)\" OR \"Tom AND Jerry\""),
            Expr::Or(vec![e("Movie (2019)"), Expr::Leaf(p("Tom AND Jerry"))])
        );
        assert_eq!(e("|\"Movie (2019)\".mkv| OR x"), Expr::Or(vec![e("|Movie (2019).mkv|"), e("x")]));
        assert_eq!(e("\"a\"OR\"b\""), Expr::Or(vec![e("a"), e("b")]));
    }

    #[test]
    fn expr_errors() {
        let cases = [
            ("(a OR b", PatternError::UnbalancedParentheses),
            ("a OR b)", PatternError::UnbalancedParentheses),
            ("a OR", PatternError::MissingOperand),
            ("AND b", PatternError::MissingOperand),
            ("a OR OR b", PatternError::MissingOperand),
            ("() OR a", PatternError::MissingOperand),
            ("\"\" OR a", PatternError::MissingOperand),
            ("(a) b OR c", PatternError::MissingOperator),
            ("a OR (b) (c)", PatternError::MissingOperator),
            ("(a (b) OR c)", PatternError::MissingOperator),
            ("a OR \"b", PatternError::UnterminatedQuote),
        ];
        for (input, error) in cases {
            assert_eq!(Expr::parse(input), Err(error), "{input:?}");
        }
    }

    #[test]
    fn expr_to_sql() {
        let mut likes = Vec::new();
        let sql = e("(a OR |b) AND c|").to_sql("col", &mut likes);
        assert_eq!(sql, r"((col LIKE ? ESCAPE '\' OR col LIKE ? ESCAPE '\') AND col LIKE ? ESCAPE '\')");
        assert_eq!(likes, ["%a%", "b%", "%c"]);
        assert!(e("").is_empty());
        assert!(!e("* OR a").is_empty());
    }
}
