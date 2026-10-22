//! Deciding what installing a staged extension would do (bd-rfz4e3bw).
//!
//! A port of Quarto 1's `versionMessage` (`install.ts:458-512`): the
//! staged and installed versions decide between Install, Update,
//! Revert, and No Change, and what the confirmation table shows as
//! `from -> to`. The table is the user's only preview of an install, so
//! it keeps Q1's exact vocabulary.

use semver::Version;

/// Shown in place of a version on the "from" side when the installed
/// copy has none (Q1 `kUnversionedFrom`).
pub const UNVERSIONED_FROM: &str = "(?)";
/// Shown in place of a version on the "to" side when the staged copy
/// has none (Q1 `kUnversionedTo`).
pub const UNVERSIONED_TO: &str = "(?)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Install,
    Update,
    Revert,
    NoChange,
}

impl Action {
    /// The label in the table (`[Install]`) and in `--json` output.
    pub fn label(self) -> &'static str {
        match self {
            Action::Install => "Install",
            Action::Update => "Update",
            Action::Revert => "Revert",
            Action::NoChange => "No Change",
        }
    }

    /// Wire string for `--json`.
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Install => "install",
            Action::Update => "update",
            Action::Revert => "revert",
            Action::NoChange => "no-change",
        }
    }
}

/// The action and the `from`/`to` columns for one extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub action: Action,
    /// Empty when there is nothing to show (an Install, or No Change).
    pub from: String,
    pub to: String,
}

/// Compare a staged extension's version against the installed copy's.
///
/// `installed` is `None` when nothing is installed at the destination,
/// and `Some(None)` when something is installed but has no (parseable)
/// version.
pub fn compare(staged: Option<&str>, installed: Option<Option<&str>>) -> Change {
    let staged = staged.and_then(coerce);
    let Some(installed) = installed else {
        return Change {
            action: Action::Install,
            from: String::new(),
            to: staged.map(|v| v.to_string()).unwrap_or_default(),
        };
    };
    let installed = installed.and_then(coerce);

    let (action, from, to) = match (staged, installed) {
        (Some(to), Some(from)) => match to.cmp(&from) {
            std::cmp::Ordering::Equal => (Action::NoChange, String::new(), String::new()),
            std::cmp::Ordering::Greater => (Action::Update, from.to_string(), to.to_string()),
            std::cmp::Ordering::Less => (Action::Revert, from.to_string(), to.to_string()),
        },
        (Some(to), None) => (Action::Update, UNVERSIONED_FROM.into(), to.to_string()),
        (None, Some(from)) => (Action::Update, from.to_string(), UNVERSIONED_TO.into()),
        // Q1 cannot tell two unversioned copies apart, so it always
        // reports an Update — kept for parity.
        (None, None) => (
            Action::Update,
            UNVERSIONED_FROM.into(),
            UNVERSIONED_TO.into(),
        ),
    };
    Change { action, from, to }
}

/// Leniently read a version, as npm's `semver.coerce` does.
///
/// Q1 reads `version:` through `coerce` (`extension.ts:813`), so
/// extension authors write things the strict grammar rejects: `1.2`,
/// `v1`, `1.0.0-beta` (the prerelease is dropped). The rule: take the
/// first run of digits that is not preceded by a digit, then up to two
/// more `.`-separated numeric parts; missing parts are zero. A string
/// with no digits has no version.
pub fn coerce(raw: &str) -> Option<Version> {
    let bytes = raw.as_bytes();
    let start = bytes.iter().position(u8::is_ascii_digit)?;
    let mut parts = [0u64; 3];
    let mut i = start;
    for (n, part) in parts.iter_mut().enumerate() {
        if n > 0 {
            // A further part needs a `.` followed by a digit.
            if bytes.get(i) != Some(&b'.') || !bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                break;
            }
            i += 1;
        }
        let digits_start = i;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        // npm caps each part at 16 digits; past that it is not a
        // version, and `u64` could not hold it anyway.
        if i - digits_start > 16 {
            return None;
        }
        *part = raw[digits_start..i].parse().ok()?;
    }
    Some(Version::new(parts[0], parts[1], parts[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Option<String> {
        coerce(s).map(|v| v.to_string())
    }

    #[test]
    fn coerce_matches_npm_on_the_forms_extensions_use() {
        assert_eq!(v("1.2.3").as_deref(), Some("1.2.3"));
        assert_eq!(v("1.2").as_deref(), Some("1.2.0"));
        assert_eq!(v("1").as_deref(), Some("1.0.0"));
        assert_eq!(v("v1").as_deref(), Some("1.0.0"));
        assert_eq!(v("v2.0.1").as_deref(), Some("2.0.1"));
        assert_eq!(v("1.0.0-beta.3").as_deref(), Some("1.0.0"));
        assert_eq!(v("1.2.3.4").as_deref(), Some("1.2.3"));
        assert_eq!(v("version 3.1 final").as_deref(), Some("3.1.0"));
        assert_eq!(v("1.").as_deref(), Some("1.0.0"));
    }

    #[test]
    fn coerce_has_no_version_without_digits() {
        assert_eq!(v(""), None);
        assert_eq!(v("latest"), None);
        assert_eq!(v("12345678901234567"), None, "more than 16 digits");
    }

    #[test]
    fn nothing_installed_is_an_install() {
        let c = compare(Some("1.0.0"), None);
        assert_eq!(c.action, Action::Install);
        assert_eq!((c.from.as_str(), c.to.as_str()), ("", "1.0.0"));
        assert_eq!(compare(None, None).to, "");
    }

    #[test]
    fn equal_versions_are_no_change() {
        let c = compare(Some("1.2"), Some(Some("1.2.0")));
        assert_eq!(c.action, Action::NoChange);
        assert_eq!((c.from.as_str(), c.to.as_str()), ("", ""));
    }

    #[test]
    fn higher_is_update_and_lower_is_revert() {
        let up = compare(Some("1.1.0"), Some(Some("1.0.0")));
        assert_eq!(up.action, Action::Update);
        assert_eq!((up.from.as_str(), up.to.as_str()), ("1.0.0", "1.1.0"));

        let down = compare(Some("1.0.0"), Some(Some("1.1.0")));
        assert_eq!(down.action, Action::Revert);
        assert_eq!((down.from.as_str(), down.to.as_str()), ("1.1.0", "1.0.0"));
    }

    #[test]
    fn any_unversioned_side_is_an_update() {
        let c = compare(Some("1.0.0"), Some(None));
        assert_eq!(
            (c.action, c.from.as_str()),
            (Action::Update, UNVERSIONED_FROM)
        );

        let c = compare(None, Some(Some("1.0.0")));
        assert_eq!((c.action, c.to.as_str()), (Action::Update, UNVERSIONED_TO));

        let c = compare(None, Some(None));
        assert_eq!(c.action, Action::Update);

        // An unparseable version counts as no version.
        let c = compare(Some("latest"), Some(Some("1.0.0")));
        assert_eq!((c.action, c.to.as_str()), (Action::Update, UNVERSIONED_TO));
    }
}
