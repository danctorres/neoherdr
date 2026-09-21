use std::borrow::Cow;

use crossterm::event::{KeyCode, KeyModifiers};

use crate::{
    config::{ActionKeybinds, IndexedKeybind, Keybinds},
    input::TerminalKey,
};

pub(crate) type KeybindHelpEntry = (String, Cow<'static, str>);
pub(crate) type KeybindHelpGroup = (Cow<'static, str>, Vec<KeybindHelpEntry>);

pub(crate) fn keybind_help_text_char(key: &TerminalKey) -> Option<char> {
    if !key.modifiers.difference(KeyModifiers::SHIFT).is_empty() {
        return None;
    }
    if let Some(character) = key.shifted_codepoint.and_then(char::from_u32) {
        return Some(character);
    }
    let KeyCode::Char(character) = key.code else {
        return None;
    };
    Some(character)
}

fn entry(key: impl Into<String>, label: &'static str) -> KeybindHelpEntry {
    (key.into(), Cow::Borrowed(label))
}

fn binding_label(bindings: &ActionKeybinds) -> String {
    bindings.label().unwrap_or_else(|| "unset".to_owned())
}

fn indexed_label(bindings: &[IndexedKeybind]) -> Option<String> {
    if bindings.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    let mut index = 0;
    while index < bindings.len() {
        if let Some(prefix) = indexed_range_prefix(&bindings[index..]) {
            parts.push(format!("{prefix}1..9"));
            index += 9;
        } else {
            parts.push(bindings[index].label.clone());
            index += 1;
        }
    }
    Some(parts.join(" / "))
}

fn indexed_range_prefix(bindings: &[IndexedKeybind]) -> Option<&str> {
    let run = bindings.get(..9)?;
    let prefix = run[0].label.strip_suffix('1')?;
    for (offset, binding) in run.iter().enumerate() {
        let digit = char::from(b'1' + offset as u8);
        if binding.label.strip_suffix(digit) != Some(prefix) {
            return None;
        }
    }
    Some(prefix)
}

/// Every built-in top-level binding as `(full label, description)`: menu
/// openers (described as `+menu`), bound actions, and indexed actions.
/// Unbound actions are omitted; they live in the menus.
fn top_level_entries(keybinds: &Keybinds) -> Vec<KeybindHelpEntry> {
    let mut entries = Vec::new();
    for group in &keybinds.groups {
        if let Some(label) = group.opener.label() {
            entries.push((label, Cow::Owned(format!("+{}", group.description))));
        }
    }
    for (bindings, action) in super::keybindings::flat_action_bindings(keybinds) {
        if let Some(label) = bindings.label() {
            entries.push((label, Cow::Borrowed(action.description())));
        }
    }
    for (bindings, description) in [
        (&keybinds.switch_tab, "switch tab 1-9"),
        (&keybinds.switch_workspace, "switch workspace 1-9"),
        (&keybinds.focus_agent, "focus agent 1-9"),
    ] {
        if let Some(label) = indexed_label(bindings) {
            entries.push((label, Cow::Borrowed(description)));
        }
    }
    entries
}

/// Ungrouped custom commands as `(full label, description)`.
fn custom_command_entries(keybinds: &Keybinds) -> Vec<KeybindHelpEntry> {
    keybinds
        .custom_commands
        .iter()
        .filter(|binding| binding.group.is_none())
        .map(|binding| {
            let description = binding
                .description
                .clone()
                .map(Cow::Owned)
                .unwrap_or(Cow::Borrowed("custom command"));
            (binding.label.clone(), description)
        })
        .collect()
}

/// The which-key entries shown on entering prefix mode, keyed by the key
/// pressed after the prefix.
pub(crate) fn prefix_menu_entries(keybinds: &Keybinds) -> Vec<KeybindHelpEntry> {
    top_level_entries(keybinds)
        .into_iter()
        .chain(custom_command_entries(keybinds))
        .filter_map(|(label, description)| {
            let parts = label
                .split(" / ")
                .filter_map(|part| part.strip_prefix("prefix+"))
                .collect::<Vec<_>>();
            (!parts.is_empty()).then(|| (parts.join(" / "), description))
        })
        .collect()
}

/// Ids of every menu reachable from prefix mode: built-in menus, then
/// user-defined `type = "group"` menus.
fn menu_ids(keybinds: &Keybinds) -> Vec<(String, String, String)> {
    keybinds
        .groups
        .iter()
        .map(|group| {
            (
                group.id.clone(),
                group.description.clone(),
                binding_label(&group.opener),
            )
        })
        .collect()
}

/// Every keybind, grouped for the help panel. Menu members whose action
/// `keep` rejects are left out, as they are in the which-key hint.
pub(crate) fn keybind_help_groups(
    keybinds: &Keybinds,
    prefixes: &[crate::config::KeyCombo],
    keep: impl Fn(&crate::config::KeyGroupAction) -> bool,
) -> Vec<KeybindHelpGroup> {
    let mut top = vec![entry(
        crate::config::format_prefix_combos(prefixes),
        "prefix mode",
    )];
    top.extend(top_level_entries(keybinds));
    let mut groups = vec![
        (Cow::Borrowed("global"), top),
        (
            Cow::Borrowed("navigation"),
            vec![
                entry("esc", "back"),
                entry(
                    format!(
                        "{} / {}",
                        binding_label(&keybinds.navigate.workspace_up),
                        binding_label(&keybinds.navigate.workspace_down)
                    ),
                    "workspace list",
                ),
                entry(
                    format!(
                        "{} / {} / {} / {} / left / right",
                        binding_label(&keybinds.navigate.pane_left),
                        binding_label(&keybinds.navigate.pane_down),
                        binding_label(&keybinds.navigate.pane_up),
                        binding_label(&keybinds.navigate.pane_right)
                    ),
                    "move focus",
                ),
                entry("tab / shift+tab", "cycle pane"),
                entry("enter", "open workspace"),
                entry("1..9", "switch workspace"),
            ],
        ),
    ];
    for (id, description, opener) in menu_ids(keybinds) {
        let entries = crate::input::group_entries_where(keybinds, &id, &keep);
        if !entries.is_empty() {
            groups.push((
                Cow::Owned(format!("{description} menu ({opener})")),
                entries,
            ));
        }
    }
    let custom = custom_command_entries(keybinds);
    if !custom.is_empty() {
        groups.push((Cow::Borrowed("custom"), custom));
    }
    groups
}

pub(crate) fn filter_keybind_help_groups(
    groups: Vec<KeybindHelpGroup>,
    query: &str,
) -> Vec<KeybindHelpGroup> {
    if query.is_empty() {
        return groups;
    }
    let query = query.to_lowercase();
    groups
        .into_iter()
        .filter_map(|(group, entries)| {
            let entries = entries
                .into_iter()
                .filter(|(key, label)| {
                    key.to_lowercase().contains(&query) || label.to_lowercase().contains(&query)
                })
                .collect::<Vec<_>>();
            (!entries.is_empty()).then_some((group, entries))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn groups() -> Vec<KeybindHelpGroup> {
        vec![
            (
                Cow::Borrowed("workspaces / tabs"),
                vec![entry("w", "workspace navigation"), entry("c", "new tab")],
            ),
            (
                Cow::Borrowed("panes"),
                vec![entry("v", "split vertical"), entry("x", "close pane")],
            ),
        ]
    }

    #[test]
    fn filter_matches_labels_and_shortcuts_case_insensitively() {
        let filtered = filter_keybind_help_groups(groups(), "WoRk");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].1[0].1, "workspace navigation");

        let filtered = filter_keybind_help_groups(groups(), "x");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].1[0].1, "close pane");
        assert!(filter_keybind_help_groups(groups(), "panes").is_empty());
    }

    #[test]
    fn help_lists_every_configured_prefix() {
        let groups = keybind_help_groups(
            &Keybinds::default(),
            &[
                (KeyCode::Char(' '), KeyModifiers::CONTROL),
                (KeyCode::Char('s'), KeyModifiers::CONTROL),
            ],
        );
        let global = &groups[0].1;
        assert_eq!(global[0].0, "ctrl+space / ctrl+s");
        assert_eq!(global[0].1, "prefix mode");
    }
}
