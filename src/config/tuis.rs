use crate::popup_size::PopupSize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TuiKind {
    #[default]
    Popup,
    Tab,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub(crate) struct TuiConfig {
    pub id: String,
    pub key: String,
    pub title: String,
    pub description: Option<String>,
    pub command: Vec<String>,
    pub platforms: Option<Vec<String>>,
    #[serde(rename = "type")]
    pub kind: TuiKind,
    pub width: Option<PopupSize>,
    pub height: Option<PopupSize>,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            key: String::new(),
            title: String::new(),
            description: None,
            command: Vec::new(),
            platforms: None,
            kind: TuiKind::Popup,
            width: None,
            height: None,
        }
    }
}

/// The keys a `[[tui]]` table may contain; anything else is reported.
const TUI_FIELDS: &[&str] = &[
    "id",
    "key",
    "title",
    "description",
    "command",
    "platforms",
    "type",
    "width",
    "height",
];

/// Validates the `[[tui]]` entries of `config.toml`.
///
/// `value` is the top-level `tui` key, if present. Each entry is validated
/// independently: an invalid entry is skipped with a diagnostic naming its
/// field path (`tui[0].key`) and the remaining entries still load.
pub(crate) fn parse_tuis(value: Option<&toml::Value>) -> (Vec<TuiConfig>, Vec<String>) {
    let Some(value) = value else {
        return (Vec::new(), Vec::new());
    };
    let Some(entries) = value.as_array() else {
        return (
            Vec::new(),
            vec!["invalid tui config: tui must be a list of [[tui]] tables; ignoring tui".into()],
        );
    };
    let mut result = Vec::new();
    let mut diagnostics = Vec::new();
    let mut ids = std::collections::HashSet::new();
    let platform = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "windows"
    };
    for (index, entry) in entries.iter().enumerate() {
        let field = format!("tui[{index}]");
        if let Some(table) = entry.as_table() {
            for key in table
                .keys()
                .filter(|key| !TUI_FIELDS.contains(&key.as_str()))
            {
                diagnostics.push(format!(
                    "unknown config key {field}.{}; ignoring key",
                    format_key(key)
                ));
            }
        }
        let tui: TuiConfig = match entry.clone().try_into() {
            Ok(tui) => tui,
            Err(err) => {
                diagnostics.push(format!("invalid tui[{index}]: {err}"));
                continue;
            }
        };
        if tui.id.trim().is_empty()
            || tui.title.trim().is_empty()
            || tui.key.trim().is_empty()
            || !tui
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            diagnostics.push(format!("{field} requires id, key, and title"));
            continue;
        }
        if tui.key.contains('+') || tui.key.chars().count() != 1 {
            diagnostics.push(format!("{field}.key must be one submenu-local key"));
            continue;
        }
        if tui.command.is_empty() || tui.command.iter().any(|arg| arg.is_empty()) {
            diagnostics.push(format!(
                "{field}.command must contain non-empty argv values"
            ));
            continue;
        }
        if !ids.insert(tui.id.clone()) {
            diagnostics.push(format!("duplicate tui id: {}", tui.id));
            continue;
        }
        if let Some(platforms) = &tui.platforms {
            if platforms
                .iter()
                .any(|platform| !matches!(platform.as_str(), "linux" | "macos" | "windows"))
            {
                diagnostics.push(format!("{field}.platforms contains an unknown platform"));
                continue;
            }
        }
        if tui
            .platforms
            .as_ref()
            .is_some_and(|platforms| !platforms.iter().any(|p| p == platform))
        {
            diagnostics.push(format!("{field} is unsupported on {platform}"));
            continue;
        }
        result.push(tui);
    }
    (result, diagnostics)
}

/// A `[[tui]]` key as written in a TOML path: bare when it can be, quoted
/// otherwise.
fn format_key(key: &str) -> String {
    if !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        key.to_owned()
    } else {
        toml::Value::String(key.to_owned()).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_tuis_str(content: &str) -> (Vec<TuiConfig>, Vec<String>) {
        let value: toml::Value = content.parse().unwrap();
        parse_tuis(value.get("tui"))
    }

    /// The copy-paste recipe documented in configuration.mdx.
    const RECIPE: &str = include_str!("../../tests/fixtures/tui-recipe.toml");
    const CONFIGURATION_DOCS: &str =
        include_str!("../../docs/next/website/src/content/docs/configuration.mdx");

    /// Returns the first ```toml block after the `### Example recipe` heading.
    fn documented_recipe() -> &'static str {
        let (_, after_heading) = CONFIGURATION_DOCS
            .split_once("\n### Example recipe\n")
            .expect("configuration.mdx has an `### Example recipe` heading");
        let (_, block) = after_heading
            .split_once("```toml\n")
            .expect("the example recipe has a ```toml block");
        let (block, _) = block
            .split_once("```")
            .expect("the example recipe block is closed");
        block
    }

    #[test]
    fn documented_recipe_matches_fixture_verbatim() {
        assert_eq!(documented_recipe(), RECIPE);
    }

    #[test]
    fn documented_recipe_parses_without_diagnostics() {
        let (tuis, diagnostics) = parse_tuis_str(RECIPE);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let entries: Vec<_> = tuis
            .iter()
            .map(|tui| (tui.id.as_str(), tui.key.as_str(), tui.command.clone()))
            .collect();
        assert_eq!(
            entries,
            vec![
                ("lazygit", "g", vec!["lazygit".to_string()]),
                ("gh-dash", "d", vec!["gh".to_string(), "dash".to_string()]),
                ("yazi", "f", vec!["yazi".to_string()]),
                ("btop", "b", vec!["btop".to_string()]),
            ]
        );
        assert!(tuis.iter().all(|tui| tui.kind == TuiKind::Popup));
    }

    #[test]
    fn missing_tui_key_yields_no_entries_and_no_diagnostics() {
        let (tuis, diagnostics) = parse_tuis(None);
        assert!(tuis.is_empty());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn non_array_tui_key_is_reported() {
        let (tuis, diagnostics) = parse_tuis_str("tui = 5\n");
        assert!(tuis.is_empty());
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("[[tui]]"), "{diagnostics:?}");
    }

    #[test]
    fn diagnostics_keep_indexed_field_paths() {
        let (tuis, diagnostics) = parse_tuis_str(
            r#"[[tui]]
id = "ok"
key = "o"
title = "Ok"
command = ["ok"]

[[tui]]
id = "bad-key"
key = "prefix+g"
title = "Bad"
command = ["bad"]

[[tui]]
id = "bad-command"
key = "c"
title = "Bad"
command = [""]
"#,
        );
        assert_eq!(tuis.len(), 1);
        assert_eq!(
            diagnostics,
            vec![
                "tui[1].key must be one submenu-local key".to_string(),
                "tui[2].command must contain non-empty argv values".to_string(),
            ]
        );
    }

    #[test]
    fn parses_valid_tui_entries() {
        let value: TuiConfig = toml::from_str(
            r#"id = "git"
            key = "g"
title = "Git"
command = ["lazygit", "--use-config"]
"#,
        )
        .unwrap();
        assert_eq!(value.command[1], "--use-config");
    }

    #[test]
    fn parses_launch_type_with_popup_default() {
        let omitted: TuiConfig = toml::from_str(
            r#"id = "git"
key = "g"
title = "Git"
command = ["lazygit"]
"#,
        )
        .unwrap();
        assert_eq!(omitted.kind, TuiKind::Popup);

        let popup: TuiConfig = toml::from_str(
            r#"id = "git"
key = "g"
title = "Git"
command = ["lazygit"]
type = "popup"
"#,
        )
        .unwrap();
        assert_eq!(popup.kind, TuiKind::Popup);

        let tab: TuiConfig = toml::from_str(
            r#"id = "nvim"
key = "n"
title = "Nvim"
command = ["nvim"]
type = "tab"
"#,
        )
        .unwrap();
        assert_eq!(tab.kind, TuiKind::Tab);
    }

    #[test]
    fn rejects_unknown_launch_type() {
        let result: Result<TuiConfig, _> = toml::from_str(
            r#"id = "git"
key = "g"
title = "Git"
command = ["lazygit"]
type = "fullscreen"
"#,
        );
        assert!(result.is_err(), "unknown type must be rejected");
    }

    #[test]
    fn skips_duplicate_and_unsupported_entries_independently() {
        let (tuis, diagnostics) = parse_tuis_str(
            r#"[[tui]]
id = "same"
key = "prefix+g"
title = "one"
command = ["one"]

[[tui]]
id = "same"
key = "h"
title = "two"
command = ["two"]

[[tui]]
id = "other"
key = "b"
title = "other"
command = ["other"]
platforms = ["not-a-platform"]
"#,
        );
        assert_eq!(tuis.len(), 1);
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn skips_unknown_launch_type_independently() {
        let (tuis, diagnostics) = parse_tuis_str(
            r#"[[tui]]
id = "bad"
key = "x"
title = "Bad"
command = ["x"]
type = "fullscreen"

[[tui]]
id = "git"
key = "g"
title = "Git"
command = ["lazygit"]
"#,
        );
        assert_eq!(tuis.len(), 1);
        assert_eq!(tuis[0].id, "git");
        assert_eq!(tuis[0].kind, TuiKind::Popup);
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.contains("invalid tui[0]")),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn unknown_keys_inside_tui_entries_are_reported() {
        let (tuis, diagnostics) = parse_tuis_str(
            r#"[[tui]]
id = "git"
key = "g"
title = "Git"
command = ["lazygit"]
platfroms = ["windows"]
"widht.x" = "80%"
"#,
        );
        assert_eq!(tuis.len(), 1);
        assert_eq!(
            diagnostics,
            vec![
                "unknown config key tui[0].platfroms; ignoring key".to_string(),
                "unknown config key tui[0].\"widht.x\"; ignoring key".to_string(),
            ]
        );
    }
}
