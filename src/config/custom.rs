use crate::popup_size::PopupSize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CustomKind {
    #[default]
    Popup,
    Tab,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub(crate) struct CustomEntry {
    pub id: String,
    pub key: String,
    pub title: String,
    pub description: Option<String>,
    pub command: Vec<String>,
    pub platforms: Option<Vec<String>>,
    #[serde(rename = "type")]
    pub kind: CustomKind,
    pub width: Option<PopupSize>,
    pub height: Option<PopupSize>,
}

impl Default for CustomEntry {
    fn default() -> Self {
        Self {
            id: String::new(),
            key: String::new(),
            title: String::new(),
            description: None,
            command: Vec::new(),
            platforms: None,
            kind: CustomKind::Popup,
            width: None,
            height: None,
        }
    }
}

/// The keys a `[[custom]]` table may contain; anything else is reported.
const CUSTOM_FIELDS: &[&str] = &[
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

/// Validates the `[[custom]]` entries of `config.toml`.
///
/// `value` is the top-level `custom` key, if present. Each entry is validated
/// independently: an invalid entry is skipped with a diagnostic naming its
/// field path (`custom[0].key`) and the remaining entries still load.
pub(crate) fn parse_custom(value: Option<&toml::Value>) -> (Vec<CustomEntry>, Vec<String>) {
    let Some(value) = value else {
        return (Vec::new(), Vec::new());
    };
    let Some(entries) = value.as_array() else {
        return (
            Vec::new(),
            vec!["invalid custom config: custom must be a list of [[custom]] tables; ignoring custom".into()],
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
        let field = format!("custom[{index}]");
        if let Some(table) = entry.as_table() {
            for key in table
                .keys()
                .filter(|key| !CUSTOM_FIELDS.contains(&key.as_str()))
            {
                diagnostics.push(format!(
                    "unknown config key {field}.{}; ignoring key",
                    format_key(key)
                ));
            }
        }
        let custom: CustomEntry = match entry.clone().try_into() {
            Ok(custom) => custom,
            Err(err) => {
                diagnostics.push(format!("invalid custom[{index}]: {err}"));
                continue;
            }
        };
        if custom.id.trim().is_empty()
            || custom.title.trim().is_empty()
            || custom.key.trim().is_empty()
            || !custom
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            diagnostics.push(format!("{field} requires id, key, and title"));
            continue;
        }
        if custom.key.contains('+') || custom.key.chars().count() != 1 {
            diagnostics.push(format!("{field}.key must be one submenu-local key"));
            continue;
        }
        if custom.command.is_empty() || custom.command.iter().any(|arg| arg.is_empty()) {
            diagnostics.push(format!(
                "{field}.command must contain non-empty argv values"
            ));
            continue;
        }
        if !ids.insert(custom.id.clone()) {
            diagnostics.push(format!("duplicate custom id: {}", custom.id));
            continue;
        }
        if let Some(platforms) = &custom.platforms {
            if platforms
                .iter()
                .any(|platform| !matches!(platform.as_str(), "linux" | "macos" | "windows"))
            {
                diagnostics.push(format!("{field}.platforms contains an unknown platform"));
                continue;
            }
        }
        if custom
            .platforms
            .as_ref()
            .is_some_and(|platforms| !platforms.iter().any(|p| p == platform))
        {
            diagnostics.push(format!("{field} is unsupported on {platform}"));
            continue;
        }
        result.push(custom);
    }
    (result, diagnostics)
}

/// A `[[custom]]` key as written in a TOML path: bare when it can be, quoted
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

    fn parse_custom_str(content: &str) -> (Vec<CustomEntry>, Vec<String>) {
        let value: toml::Value = content.parse().unwrap();
        parse_custom(value.get("custom"))
    }

    /// The copy-paste recipe documented in configuration.mdx.
    const RECIPE: &str = include_str!("../../tests/fixtures/custom-recipe.toml");
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
        let (parsed, diagnostics) = parse_custom_str(RECIPE);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let entries: Vec<_> = parsed
            .iter()
            .map(|custom| {
                (
                    custom.id.as_str(),
                    custom.key.as_str(),
                    custom.command.clone(),
                )
            })
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
        assert!(parsed.iter().all(|custom| custom.kind == CustomKind::Popup));
    }

    #[test]
    fn missing_tui_key_yields_no_entries_and_no_diagnostics() {
        let (parsed, diagnostics) = parse_custom(None);
        assert!(parsed.is_empty());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn non_array_tui_key_is_reported() {
        let (parsed, diagnostics) = parse_custom_str("custom = 5\n");
        assert!(parsed.is_empty());
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("[[custom]]"), "{diagnostics:?}");
    }

    #[test]
    fn diagnostics_keep_indexed_field_paths() {
        let (parsed, diagnostics) = parse_custom_str(
            r#"[[custom]]
id = "ok"
key = "o"
title = "Ok"
command = ["ok"]

[[custom]]
id = "bad-key"
key = "prefix+g"
title = "Bad"
command = ["bad"]

[[custom]]
id = "bad-command"
key = "c"
title = "Bad"
command = [""]
"#,
        );
        assert_eq!(parsed.len(), 1);
        assert_eq!(
            diagnostics,
            vec![
                "custom[1].key must be one submenu-local key".to_string(),
                "custom[2].command must contain non-empty argv values".to_string(),
            ]
        );
    }

    #[test]
    fn parses_valid_tui_entries() {
        let value: CustomEntry = toml::from_str(
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
        let omitted: CustomEntry = toml::from_str(
            r#"id = "git"
key = "g"
title = "Git"
command = ["lazygit"]
"#,
        )
        .unwrap();
        assert_eq!(omitted.kind, CustomKind::Popup);

        let popup: CustomEntry = toml::from_str(
            r#"id = "git"
key = "g"
title = "Git"
command = ["lazygit"]
type = "popup"
"#,
        )
        .unwrap();
        assert_eq!(popup.kind, CustomKind::Popup);

        let tab: CustomEntry = toml::from_str(
            r#"id = "nvim"
key = "n"
title = "Nvim"
command = ["nvim"]
type = "tab"
"#,
        )
        .unwrap();
        assert_eq!(tab.kind, CustomKind::Tab);
    }

    #[test]
    fn rejects_unknown_launch_type() {
        let result: Result<CustomEntry, _> = toml::from_str(
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
        let (parsed, diagnostics) = parse_custom_str(
            r#"[[custom]]
id = "same"
key = "prefix+g"
title = "one"
command = ["one"]

[[custom]]
id = "same"
key = "h"
title = "two"
command = ["two"]

[[custom]]
id = "other"
key = "b"
title = "other"
command = ["other"]
platforms = ["not-a-platform"]
"#,
        );
        assert_eq!(parsed.len(), 1);
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn skips_unknown_launch_type_independently() {
        let (parsed, diagnostics) = parse_custom_str(
            r#"[[custom]]
id = "bad"
key = "x"
title = "Bad"
command = ["x"]
type = "fullscreen"

[[custom]]
id = "git"
key = "g"
title = "Git"
command = ["lazygit"]
"#,
        );
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "git");
        assert_eq!(parsed[0].kind, CustomKind::Popup);
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.contains("invalid custom[0]")),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn unknown_keys_inside_tui_entries_are_reported() {
        let (parsed, diagnostics) = parse_custom_str(
            r#"[[custom]]
id = "git"
key = "g"
title = "Git"
command = ["lazygit"]
platfroms = ["windows"]
"widht.x" = "80%"
"#,
        );
        assert_eq!(parsed.len(), 1);
        assert_eq!(
            diagnostics,
            vec![
                "unknown config key custom[0].platfroms; ignoring key".to_string(),
                "unknown config key custom[0].\"widht.x\"; ignoring key".to_string(),
            ]
        );
    }
}
