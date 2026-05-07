use once_cell::sync::Lazy;
use regex::Regex;

pub(super) fn compile_static_regex(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(value) => value,
        Err(err) => {
            tracing::error!(pattern, error = %err, "failed to compile code navigation regex");
            disabled_regex()
        }
    }
}

fn disabled_regex() -> Regex {
    static DISABLED_REGEX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"$^").unwrap_or_else(|err| {
            panic!("internal disabled code navigation regex failed to compile: {err}")
        })
    });
    DISABLED_REGEX.clone()
}
