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

pub(crate) fn load_tuis() -> (Vec<TuiConfig>, Vec<String>) {
    let path = tui_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return (Vec::new(), Vec::new());
        }
        Err(err) => return (Vec::new(), vec![format!("tuis.toml read error: {err}")]),
    };
    parse_tuis(&content)
}

fn parse_tuis(content: &str) -> (Vec<TuiConfig>, Vec<String>) {
    let value = match content.parse::<toml::Value>() {
        Ok(value) => value,
        Err(err) => return (Vec::new(), vec![format!("tuis.toml parse error: {err}")]),
    };
    let Some(entries) = value.get("tui").and_then(toml::Value::as_array) else {
        return (
            Vec::new(),
            vec!["tuis.toml must contain [[tui]] entries".into()],
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
        let tui: TuiConfig = match entry.clone().try_into() {
            Ok(tui) => tui,
            Err(err) => {
                diagnostics.push(format!("invalid tui[{index}]: {err}"));
                continue;
            }
        };
        let field = format!("tui[{index}]");
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

/// `tuis.toml` lives next to the main herdr config file, so it follows the
/// same `HERDR_CONFIG_PATH`/XDG/platform resolution as `config.toml`.
fn tui_path() -> std::path::PathBuf {
    super::config_path().with_file_name("tuis.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let (tuis, diagnostics) = parse_tuis(
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
        let (tuis, diagnostics) = parse_tuis(
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
}
