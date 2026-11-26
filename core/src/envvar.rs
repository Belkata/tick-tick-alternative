//! Environment variables, with the pre-rename `TT_*` names as a fallback.
//!
//! The project was renamed to Utre: `UTRE_DB` replaces `TT_DB`, and so on. Anything that
//! still exports the old name keeps working; the new name wins when both are set.

use std::ffi::OsString;

/// `$UTRE_<suffix>`, else the legacy `$TT_<suffix>`. Pass the suffix only (`"DB"`).
pub fn var_os(suffix: &str) -> Option<OsString> {
    lookup(suffix, |k| std::env::var_os(k))
}

/// Like [`var_os`], but only for values that are valid Unicode.
pub fn var(suffix: &str) -> Option<String> {
    var_os(suffix).and_then(|v| v.into_string().ok())
}

fn lookup(suffix: &str, get: impl Fn(&str) -> Option<OsString>) -> Option<OsString> {
    get(&format!("UTRE_{suffix}")).or_else(|| get(&format!("TT_{suffix}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn new_name_wins_and_old_name_falls_back() {
        let both = env(&[("UTRE_DB", "new"), ("TT_DB", "old")]);
        assert_eq!(lookup("DB", both), Some("new".into()));
        let old = env(&[("TT_DB", "old")]);
        assert_eq!(lookup("DB", old), Some("old".into()));
        assert_eq!(lookup("DB", env(&[])), None);
    }
}
