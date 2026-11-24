//! Evaluator for the `condition = "..."` expressions on pipeline steps.
//!
//! Grammar (lowest to highest precedence):
//!
//! ```text
//! expr       := and ("or" and)*
//! and        := not ("and" not)*
//! not        := "not" not | comparison
//! comparison := value [ ("==" | "!=" | "<" | "<=" | ">" | ">=") value
//!                     | ["not"] "in" "[" value ("," value)* "]" ]
//! value      := ( string | number | {placeholder} | "true" | "false" | "(" expr ")" )
//!               ( "." ("startswith" | "endswith" | "contains") "(" value ")" )?
//! ```
//!
//! The expression is tokenized *before* placeholders are substituted, so a file
//! name containing quotes or `or` cannot change the structure of the condition.
//! If both sides of a comparison parse as numbers they are compared numerically,
//! otherwise as strings.

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Str(String),
    Num(String),
    Placeholder(String),
    Word(String),
    Op(&'static str),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Dot,
}

fn tokenize(src: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            c if c.is_whitespace() => i += 1,
            '\'' | '"' => {
                let quote = c;
                let mut s = String::new();
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err(format!("unterminated string in condition: {src}")),
                        Some('\\') if i + 1 < chars.len() => {
                            s.push(chars[i + 1]);
                            i += 2;
                        }
                        Some(&q) if q == quote => {
                            i += 1;
                            break;
                        }
                        Some(&ch) => {
                            s.push(ch);
                            i += 1;
                        }
                    }
                }
                toks.push(Tok::Str(s));
            }
            '{' => {
                let end = chars[i..]
                    .iter()
                    .position(|&ch| ch == '}')
                    .ok_or_else(|| format!("unclosed '{{' in condition: {src}"))?;
                toks.push(Tok::Placeholder(chars[i..=i + end].iter().collect()));
                i += end + 1;
            }
            '(' => {
                toks.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                toks.push(Tok::RParen);
                i += 1;
            }
            '[' => {
                toks.push(Tok::LBracket);
                i += 1;
            }
            ']' => {
                toks.push(Tok::RBracket);
                i += 1;
            }
            ',' => {
                toks.push(Tok::Comma);
                i += 1;
            }
            '.' => {
                toks.push(Tok::Dot);
                i += 1;
            }
            '=' | '!' | '<' | '>' => {
                let two: String = chars[i..].iter().take(2).collect();
                let op = match two.as_str() {
                    "==" => Some("=="),
                    "!=" => Some("!="),
                    "<=" => Some("<="),
                    ">=" => Some(">="),
                    _ => None,
                };
                if let Some(op) = op {
                    toks.push(Tok::Op(op));
                    i += 2;
                } else if c == '<' {
                    toks.push(Tok::Op("<"));
                    i += 1;
                } else if c == '>' {
                    toks.push(Tok::Op(">"));
                    i += 1;
                } else {
                    return Err(format!("unexpected '{c}' in condition: {src}"));
                }
            }
            c if c.is_ascii_digit() || c == '-' => {
                let start = i;
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                toks.push(Tok::Num(chars[start..i].iter().collect()));
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                toks.push(Tok::Word(chars[start..i].iter().collect()));
            }
            other => return Err(format!("unexpected '{other}' in condition: {src}")),
        }
    }
    Ok(toks)
}

struct Parser<'a> {
    toks: Vec<Tok>,
    pos: usize,
    subst: &'a dyn Fn(&str) -> String,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn eat_word(&mut self, word: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Word(w)) if w == word) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tok: Tok) -> Result<(), String> {
        match self.next() {
            Some(ref t) if *t == tok => Ok(()),
            other => Err(format!("expected {tok:?}, found {other:?}")),
        }
    }

    fn expr(&mut self) -> Result<bool, String> {
        let mut result = self.and()?;
        while self.eat_word("or") {
            let rhs = self.and()?;
            result = result || rhs;
        }
        Ok(result)
    }

    fn and(&mut self) -> Result<bool, String> {
        let mut result = self.not()?;
        while self.eat_word("and") {
            let rhs = self.not()?;
            result = result && rhs;
        }
        Ok(result)
    }

    fn not(&mut self) -> Result<bool, String> {
        if self.eat_word("not") {
            return Ok(!self.not()?);
        }
        self.comparison()
    }

    fn comparison(&mut self) -> Result<bool, String> {
        let lhs = self.value()?;

        let negate_in = matches!(self.peek(), Some(Tok::Word(w)) if w == "not")
            && matches!(self.toks.get(self.pos + 1), Some(Tok::Word(w)) if w == "in");
        if negate_in {
            self.pos += 1;
        }
        if self.eat_word("in") {
            self.expect(Tok::LBracket)?;
            let mut found = false;
            if self.peek() != Some(&Tok::RBracket) {
                loop {
                    if self.value()? == lhs {
                        found = true;
                    }
                    if self.peek() == Some(&Tok::Comma) {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
            }
            self.expect(Tok::RBracket)?;
            return Ok(found != negate_in);
        }

        if let Some(Tok::Op(op)) = self.peek().cloned() {
            self.pos += 1;
            let rhs = self.value()?;
            return Ok(compare(&lhs, op, &rhs));
        }

        // A lone value is a truth test.
        Ok(!(lhs.is_empty() || lhs == "false" || lhs == "0"))
    }

    fn value(&mut self) -> Result<String, String> {
        let base = match self.next() {
            Some(Tok::Str(s)) | Some(Tok::Placeholder(s)) => (self.subst)(&s),
            Some(Tok::Num(n)) => n,
            Some(Tok::Word(w)) if w == "true" || w == "false" => w,
            Some(Tok::LParen) => {
                let inner = self.expr()?;
                self.expect(Tok::RParen)?;
                inner.to_string()
            }
            other => return Err(format!("expected a value, found {other:?}")),
        };

        if self.peek() != Some(&Tok::Dot) {
            return Ok(base);
        }
        self.pos += 1;
        let method = match self.next() {
            Some(Tok::Word(w)) => w,
            other => return Err(format!("expected a method name after '.', found {other:?}")),
        };
        self.expect(Tok::LParen)?;
        let arg = self.value()?;
        self.expect(Tok::RParen)?;
        let result = match method.as_str() {
            "startswith" => base.starts_with(&arg),
            "endswith" => base.ends_with(&arg),
            "contains" => base.contains(&arg),
            other => {
                return Err(format!(
                    "unknown method '.{other}()' (expected startswith, endswith or contains)"
                ));
            }
        };
        Ok(result.to_string())
    }
}

fn compare(lhs: &str, op: &str, rhs: &str) -> bool {
    use std::cmp::Ordering;
    let ord = match (lhs.trim().parse::<f64>(), rhs.trim().parse::<f64>()) {
        (Ok(a), Ok(b)) => a.partial_cmp(&b),
        _ => Some(lhs.cmp(rhs)),
    };
    match (op, ord) {
        ("==", Some(o)) => o == Ordering::Equal,
        ("!=", Some(o)) => o != Ordering::Equal,
        ("<", Some(o)) => o == Ordering::Less,
        ("<=", Some(o)) => o != Ordering::Greater,
        (">", Some(o)) => o == Ordering::Greater,
        (">=", Some(o)) => o != Ordering::Less,
        // NaN compares unequal to everything.
        ("!=", None) => true,
        _ => false,
    }
}

/// Evaluate `cond`, using `subst` to expand `{placeholders}` in strings and bare
/// `{placeholder}` values. An empty condition is true.
pub fn evaluate(cond: &str, subst: &dyn Fn(&str) -> String) -> Result<bool, String> {
    if cond.trim().is_empty() {
        return Ok(true);
    }
    let toks = tokenize(cond)?;
    let mut parser = Parser {
        toks,
        pos: 0,
        subst,
    };
    let result = parser
        .expr()
        .map_err(|e| format!("{e} (in condition: {cond})"))?;
    if parser.pos < parser.toks.len() {
        return Err(format!(
            "unexpected {:?} in condition: {cond}",
            parser.toks[parser.pos]
        ));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval_with(cond: &str, vars: &[(&str, &str)]) -> Result<bool, String> {
        let vars: Vec<(String, String)> = vars
            .iter()
            .map(|(k, v)| (format!("{{{k}}}"), v.to_string()))
            .collect();
        evaluate(cond, &|s: &str| {
            let mut out = s.to_string();
            for (k, v) in &vars {
                out = out.replace(k, v);
            }
            out
        })
    }

    #[test]
    fn empty_is_true() {
        assert_eq!(eval_with("", &[]), Ok(true));
        assert_eq!(eval_with("   ", &[]), Ok(true));
    }

    #[test]
    fn numeric_comparisons() {
        let v = [("file.size", "50000")];
        assert_eq!(eval_with("{file.size} > 40000", &v), Ok(true));
        assert_eq!(eval_with("{file.size} >= 50000", &v), Ok(true));
        assert_eq!(eval_with("{file.size} < 40000", &v), Ok(false));
        assert_eq!(eval_with("{file.size} == 50000", &v), Ok(true));
        assert_eq!(eval_with("{file.size} != 50000", &v), Ok(false));
        // Numeric, not lexicographic: "9" < "10".
        assert_eq!(eval_with("9 < 10", &[]), Ok(true));
    }

    #[test]
    fn string_functions_and_membership() {
        let v = [("file.mime", "image/png"), ("file.ext", "pdf")];
        assert_eq!(
            eval_with("'{file.mime}'.startswith('image/')", &v),
            Ok(true)
        );
        assert_eq!(eval_with("'{file.mime}'.endswith('png')", &v), Ok(true));
        assert_eq!(eval_with("'{file.mime}'.contains('age')", &v), Ok(true));
        assert_eq!(eval_with("'{file.ext}' in ['pdf', 'docx']", &v), Ok(true));
        assert_eq!(eval_with("'{file.ext}' in ['zip']", &v), Ok(false));
        assert_eq!(eval_with("'{file.ext}' not in ['zip']", &v), Ok(true));
        assert_eq!(eval_with("'a' in []", &v), Ok(false));
    }

    #[test]
    fn boolean_operators_and_precedence() {
        let v = [("file.mime", "video/mp4"), ("file.size", "10")];
        assert_eq!(
            eval_with(
                "'{file.mime}'.startswith('image/') or '{file.mime}'.startswith('video/')",
                &v
            ),
            Ok(true)
        );
        assert_eq!(
            eval_with(
                "'{file.mime}'.startswith('video/') and {file.size} > 100",
                &v
            ),
            Ok(false)
        );
        // `and` binds tighter than `or`.
        assert_eq!(eval_with("1 == 1 or 1 == 2 and 1 == 2", &[]), Ok(true));
        assert_eq!(eval_with("not (1 == 1 or 2 == 2)", &[]), Ok(false));
        assert_eq!(eval_with("not 1 == 2", &[]), Ok(true));
    }

    #[test]
    fn values_with_quotes_cannot_change_structure() {
        let v = [("file.name", "x' or '1'=='1")];
        assert_eq!(eval_with("'{file.name}' == 'safe'", &v), Ok(false));
        assert_eq!(eval_with("'{file.name}'.startswith('x')", &v), Ok(true));
    }

    #[test]
    fn truthiness_of_lone_values() {
        assert_eq!(eval_with("true", &[]), Ok(true));
        assert_eq!(eval_with("false", &[]), Ok(false));
        assert_eq!(eval_with("'{flag}'", &[("flag", "false")]), Ok(false));
        assert_eq!(eval_with("'{flag}'", &[("flag", "yes")]), Ok(true));
    }

    #[test]
    fn syntax_errors_are_reported() {
        assert!(eval_with("'abc", &[]).is_err());
        assert!(eval_with("1 ==", &[]).is_err());
        assert!(eval_with("'a'.frobnicate('b')", &[]).is_err());
        assert!(eval_with("1 == 1 garbage", &[]).is_err());
        assert!(eval_with("(1 == 1", &[]).is_err());
    }
}
