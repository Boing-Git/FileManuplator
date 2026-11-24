//! Running external tools without a shell.
//!
//! Plugin `args` strings are *templates*, and they are turned into an argv list
//! in two separate stages:
//!
//! 1. **Tokenize the template.** Quotes and spaces in the plugin file are
//!    structural: they decide where one argument ends and the next begins.
//! 2. **Substitute `{placeholders}` inside each token.** Values (file paths, UI
//!    widget values) are pure data at this point — whatever they contain, they
//!    stay inside the single argument they were substituted into.
//!
//! Doing it in this order is what makes the engine safe. A file named
//! `my friend's clip.mp4` is one argument, and a text input containing
//! `1; rm -rf ~` is one argument that ffmpeg simply rejects. Neither can add
//! arguments or commands, because no shell ever sees the result.

use crate::engine::format_str_with_state;
use crate::types::FileContext;
use std::collections::HashMap;
use std::process::Command;

/// Split an args *template* into argument tokens, honouring `'single'` and
/// `"double"` quotes and backslash escapes. Quotes group, and are removed.
pub fn tokenize_args(template: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut has_token = false;
    let mut chars = template.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if has_token {
                    tokens.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            '\'' | '"' => {
                let quote = c;
                has_token = true;
                loop {
                    match chars.next() {
                        None => return Err(format!("unterminated {quote} quote in: {template}")),
                        Some('\\') if quote == '"' => match chars.next() {
                            Some(esc) => current.push(esc),
                            None => return Err(format!("trailing backslash in: {template}")),
                        },
                        Some(ch) if ch == quote => break,
                        Some(ch) => current.push(ch),
                    }
                }
            }
            '\\' => {
                has_token = true;
                match chars.next() {
                    Some(esc) => current.push(esc),
                    None => return Err(format!("trailing backslash in: {template}")),
                }
            }
            ch => {
                has_token = true;
                current.push(ch);
            }
        }
    }
    if has_token {
        tokens.push(current);
    }
    Ok(tokens)
}

/// Tokenize an args template and expand `{placeholders}` within each token.
pub fn build_argv(
    template: &str,
    ctx: &FileContext,
    state: &HashMap<String, String>,
) -> Result<Vec<String>, String> {
    Ok(tokenize_args(template)?
        .iter()
        .map(|tok| format_str_with_state(tok, ctx, state))
        .collect())
}

/// Is `program` runnable? Checks PATH entries directly rather than shelling out.
pub fn program_exists(program: &str) -> bool {
    if program.contains('/') {
        return std::path::Path::new(program).is_file();
    }
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(program);
                candidate.is_file() && is_executable(&candidate)
            })
        })
        .unwrap_or(false)
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_path: &std::path::Path) -> bool {
    true
}

/// Run `program` with `argv`, with no shell involved.
///
/// stderr is captured so a failing tool can explain itself instead of only
/// reporting an exit code.
pub fn run_program(program: &str, argv: &[String]) -> Result<(), String> {
    if !program_exists(program) {
        return Err(format!(
            "'{program}' was not found on your PATH. Install it and try again."
        ));
    }

    let output = Command::new(program)
        .args(argv)
        .output()
        .map_err(|e| format!("{program} failed to launch: {e}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail: String = stderr
        .lines()
        .rfind(|l| !l.trim().is_empty())
        .unwrap_or("no error output")
        .chars()
        .take(300)
        .collect();
    Err(format!(
        "{program} failed ({}): {detail}",
        output
            .status
            .code()
            .map(|c| format!("exit code {c}"))
            .unwrap_or_else(|| "terminated by signal".to_string())
    ))
}

/// Parse a byte size such as `4G`, `3900M`, `2K` or a plain byte count.
pub fn parse_size(text: &str) -> Result<u64, String> {
    let t = text.trim().to_uppercase();
    let t = t.strip_suffix('B').unwrap_or(&t);
    let (number, multiplier) = match t.chars().last() {
        Some('G') => (&t[..t.len() - 1], 1024u64 * 1024 * 1024),
        Some('M') => (&t[..t.len() - 1], 1024u64 * 1024),
        Some('K') => (&t[..t.len() - 1], 1024u64),
        _ => (t, 1),
    };
    let value: f64 = number
        .trim()
        .parse()
        .map_err(|_| format!("invalid size '{text}' (try 24M, 4G or a byte count)"))?;
    if value <= 0.0 {
        return Err(format!("size must be greater than zero, got '{text}'"));
    }
    Ok((value * multiplier as f64) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> FileContext {
        FileContext::from_paths(
            std::path::Path::new("/tmp/my friend's clip.mp4"),
            std::path::Path::new("/tmp/out dir/out.mp4"),
            "video/mp4",
        )
    }

    #[test]
    fn quotes_group_arguments_and_are_removed() {
        assert_eq!(
            tokenize_args("'{file.path}' -resize 50% '{output.path}'").unwrap(),
            vec!["{file.path}", "-resize", "50%", "{output.path}"]
        );
        assert_eq!(tokenize_args("a  b\tc").unwrap(), vec!["a", "b", "c"]);
        assert_eq!(tokenize_args(r#""two words""#).unwrap(), vec!["two words"]);
        assert!(tokenize_args("'unterminated").is_err());
    }

    #[test]
    fn a_path_with_an_apostrophe_stays_one_argument() {
        let argv = build_argv("-i '{file.path}' '{output.path}'", &ctx(), &HashMap::new()).unwrap();
        assert_eq!(
            argv,
            vec!["-i", "/tmp/my friend's clip.mp4", "/tmp/out dir/out.mp4"]
        );
    }

    #[test]
    fn quoting_is_no_longer_needed_for_paths_with_spaces() {
        // The template is tokenized before substitution, so `{file.path}` is
        // already a single argument whether or not the author quoted it.
        let unquoted = build_argv("-i {file.path} {output.path}", &ctx(), &HashMap::new()).unwrap();
        let quoted =
            build_argv("-i '{file.path}' '{output.path}'", &ctx(), &HashMap::new()).unwrap();
        assert_eq!(unquoted, quoted);
        assert_eq!(unquoted[1], "/tmp/my friend's clip.mp4");
    }

    #[test]
    fn a_widget_value_cannot_add_arguments_or_commands() {
        let mut state = HashMap::new();
        state.insert("duration".to_string(), "1 ; touch /tmp/pwned #".to_string());
        let argv = build_argv("-t {duration} out.mp4", &ctx(), &state).unwrap();
        // The whole injection payload is a single, inert argument.
        assert_eq!(argv, vec!["-t", "1 ; touch /tmp/pwned #", "out.mp4"]);
    }

    #[test]
    fn sizes_parse() {
        assert_eq!(parse_size("1024").unwrap(), 1024);
        assert_eq!(parse_size("2K").unwrap(), 2048);
        assert_eq!(parse_size("24M").unwrap(), 24 * 1024 * 1024);
        assert_eq!(parse_size("4G").unwrap(), 4 * 1024 * 1024 * 1024);
        assert_eq!(parse_size("3900MB").unwrap(), 3900 * 1024 * 1024);
        assert!(parse_size("").is_err());
        assert!(parse_size("-1M").is_err());
        assert!(parse_size("big").is_err());
    }

    #[test]
    fn missing_program_is_reported_clearly() {
        let err = run_program("definitely_not_a_real_binary_xyz", &[]).unwrap_err();
        assert!(err.contains("was not found on your PATH"), "{err}");
    }
}
