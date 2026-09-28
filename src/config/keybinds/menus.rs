//! Which-key menus.
//!
//! Built-in menus (`workspace`, `tab`, ...), user menus opened by
//! `[[keys.command]] type = "group"` entries, and the entries that join a
//! menu through `group = "<id>"` are all projected into one resolved
//! [`KeyGroup`] list on [`Keybinds::groups`]. Input resolution, the which-key
//! hint, and the keybind help read only that list.
//!
//! The built-in menus are themselves `[[keys.command]]` entries
//! ([`DEFAULT_MENUS`]), parsed through the same path as user entries, so a
//! user config can express any of them.

use std::borrow::Cow;
use std::sync::OnceLock;

use tracing::warn;

use super::{
    format_key_combo, normalize_key_combo, parse_binding_string, parse_key_combo, ActionKeybinds,
    BindingRegistry, BindingSource, BindingTrigger, CommandKeybindConfig, CommandKeybindType,
    Keybinds, ParsedBinding, ResolvedBinding,
};
use crate::config::Config;

pub(crate) const WORKSPACE_GROUP: &str = "workspace";
pub(crate) const TAB_GROUP: &str = "tab";
pub(crate) const PANE_GROUP: &str = "pane";
pub(crate) const AGENT_GROUP: &str = "agent";
pub(crate) const GIT_GROUP: &str = "git";
pub(crate) const SYSTEM_GROUP: &str = "system";
/// Menu holding the TUIs configured as `[[tui]]` entries in config.toml.
pub(crate) const TUI_GROUP: &str = "open";

/// What a which-key menu entry runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyGroupAction {
    Builtin(crate::input::KeybindAction),
    PluginAction(&'static str),
    /// A user `[[keys.command]]` or configured TUI entry, by index into
    /// [`Keybinds::custom_commands`].
    Command(usize),
}

/// One entry of a which-key menu. Its keys are pressed without the prefix
/// once the menu is open.
#[derive(Debug, Clone)]
pub(crate) struct KeyGroupMember {
    pub(crate) keys: ActionKeybinds,
    pub(crate) label: String,
    pub(crate) action: KeyGroupAction,
    pub(crate) description: Cow<'static, str>,
}

/// A resolved which-key menu, built in or user-defined. User entries join a
/// menu by setting `group` to its id and are listed first; a user entry with
/// the same key replaces a built-in entry.
#[derive(Debug, Clone)]
pub(crate) struct KeyGroup {
    pub(crate) id: String,
    pub(crate) description: String,
    pub(crate) opener: ActionKeybinds,
    pub(crate) members: Vec<KeyGroupMember>,
    /// Opened by a user `type = "group"` entry. Such a menu is kept even when
    /// it has no entries.
    pub(crate) user_defined: bool,
}

/// One parsed menu member, user or built in, before [`resolve_groups`]
/// merges them into [`KeyGroup`]s.
pub(super) struct MenuEntry {
    pub(super) group: String,
    pub(super) keys: ActionKeybinds,
    pub(super) action: KeyGroupAction,
    pub(super) description: Cow<'static, str>,
    pub(super) source: BindingSource,
}

/// The built-in which-key menus, written as `[[keys.command]]` entries: a
/// `type = "group"` entry per menu (opened by its `keys.<id>_menu` field),
/// then its members. Menus are named after the object they act on, and every
/// key comes from one of these rules, in order:
///
/// - one verb vocabulary: `n` new, `r` rename, `x` close/remove, `s`
///   switch/pick
/// - an entry that also has a top-level key uses that same key (`x`, `z`,
///   `\`, `-`, `;`, `y`), so learning one level teaches the other
/// - direction keys walk the item's own axis: `h`/`l` for the horizontal tab
///   bar, `j`/`k` for the vertical workspace and agent lists
/// - shift turns "go to" into "move": `H`/`L` move a tab, `J`/`K` move a
///   workspace, `:` (shifted `;`) swaps with the last pane
/// - shift undoes the lowercase verb where there is no motion: `R` removes
///   a name that `r` set
/// - the rest follow common convention: `c` clear, `e` editor, `,`
///   settings, `?` help, `q` detach
/// - agents get no per-agent keys: `n` in the agent menu opens a picker of
///   the agents installed on the runtime host, so no default key depends on
///   what is installed
///
/// A member's description defaults to its action's description.
const DEFAULT_MENUS: &str = r#"
command = [
  { type = "group", group = "workspace", description = "workspace" },
  { group = "workspace", key = "n", action = "new_workspace" },
  { group = "workspace", key = "r", action = "rename_workspace" },
  { group = "workspace", key = "x", action = "close_workspace" },
  { group = "workspace", key = "s", action = "workspace_picker" },
  { group = "workspace", key = "j", action = "next_workspace" },
  { group = "workspace", key = "k", action = "previous_workspace" },
  { group = "workspace", key = "J", action = "move_workspace_next" },
  { group = "workspace", key = "K", action = "move_workspace_previous" },
  { group = "workspace", key = "g", action = "toggle_group" },

  { type = "group", group = "tab", description = "tab" },
  { group = "tab", key = "n", action = "new_tab" },
  { group = "tab", key = "r", action = "rename_tab" },
  { group = "tab", key = "x", action = "close_tab" },
  { group = "tab", key = "h", action = "previous_tab" },
  { group = "tab", key = "l", action = "next_tab" },
  { group = "tab", key = "H", action = "move_tab_previous" },
  { group = "tab", key = "L", action = "move_tab_next" },

  { type = "group", group = "pane", description = "pane" },
  { group = "pane", key = "backslash", action = "split_vertical" },
  { group = "pane", key = "minus", action = "split_horizontal" },
  { group = "pane", key = "x", action = "close_pane" },
  { group = "pane", key = "z", action = "zoom" },
  { group = "pane", key = "semicolon", action = "last_pane" },
  { group = "pane", key = ":", action = "swap_with_focused_pane" },
  { group = "pane", key = "r", action = "rename_pane" },
  { group = "pane", key = "R", action = "clear_pane_name" },
  { group = "pane", key = "e", action = "edit_scrollback" },
  { group = "pane", key = "c", action = "clear_pane" },
  { group = "pane", key = "y", action = "copy_mode" },

  { type = "group", group = "agent", description = "agent" },
  { group = "agent", key = "n", action = "new_agent_tab", description = "new agent tab…" },
  { group = "agent", key = "a", action = "open_notification_target" },
  { group = "agent", key = "j", action = "next_agent" },
  { group = "agent", key = "k", action = "previous_agent" },

  { type = "group", group = "git", description = "git" },
  { group = "git", key = "n", action = "new_worktree" },
  { group = "git", key = "o", action = "open_worktree" },
  { group = "git", key = "x", action = "remove_worktree" },

  { type = "group", group = "system", description = "system" },
  { group = "system", key = "comma", action = "settings" },
  { group = "system", key = "r", action = "reload_config" },
  { group = "system", key = "b", action = "toggle_sidebar" },
  { group = "system", key = "?", action = "help" },
  { group = "system", key = "q", action = "detach" },

  { type = "group", group = "open", description = "open TUI" },
]
"#;

/// [`DEFAULT_MENUS`] parsed once.
pub(super) fn default_menu_entries() -> &'static [CommandKeybindConfig] {
    #[derive(serde::Deserialize)]
    struct Fragment {
        command: Vec<CommandKeybindConfig>,
    }
    static ENTRIES: OnceLock<Vec<CommandKeybindConfig>> = OnceLock::new();
    ENTRIES.get_or_init(|| match toml::from_str::<Fragment>(DEFAULT_MENUS) {
        Ok(fragment) => fragment.command,
        Err(err) => {
            tracing::error!(%err, "built-in menu definitions do not parse");
            Vec::new()
        }
    })
}

/// Position of a built-in menu among the default menus.
fn builtin_group_position(id: &str) -> Option<usize> {
    default_menu_entries()
        .iter()
        .filter(|entry| entry.action_type == CommandKeybindType::Group)
        .position(|entry| entry.group.as_deref() == Some(id))
}

/// The `keys.<id>_menu` opener of a built-in menu.
pub(super) fn builtin_group_opener<'a>(
    keybinds: &'a Keybinds,
    id: &str,
) -> Option<&'a ActionKeybinds> {
    match id {
        WORKSPACE_GROUP => Some(&keybinds.workspace_menu),
        TAB_GROUP => Some(&keybinds.tab_menu),
        PANE_GROUP => Some(&keybinds.pane_menu),
        AGENT_GROUP => Some(&keybinds.agent_menu),
        GIT_GROUP => Some(&keybinds.git_menu),
        SYSTEM_GROUP => Some(&keybinds.system_menu),
        TUI_GROUP => Some(&keybinds.tui_menu),
        _ => None,
    }
}

pub(super) fn is_builtin_group(id: &str) -> bool {
    builtin_group_position(id).is_some()
}

/// Records the menu opened by a `type = "group"` entry. Openers sharing an
/// id open the same menu; a built-in menu keeps its description and lists
/// its own opener first.
pub(super) fn add_group(
    keybinds: &mut Keybinds,
    id: &str,
    description: Option<&str>,
    opener: ActionKeybinds,
    source: BindingSource,
) {
    if let Some(group) = keybinds.groups.iter_mut().find(|group| group.id == id) {
        match source {
            BindingSource::User => group.opener.bindings.extend(opener.bindings),
            BindingSource::Default => {
                let user_bindings = std::mem::replace(&mut group.opener, opener).bindings;
                group.opener.bindings.extend(user_bindings);
                group.description = description.unwrap_or(id).to_owned();
            }
        }
        return;
    }
    keybinds.groups.push(KeyGroup {
        id: id.to_owned(),
        description: description.unwrap_or(id).to_owned(),
        opener,
        members: Vec::new(),
        user_defined: source == BindingSource::User,
    });
}

/// Whether a menu with this id exists so far: built in, or opened by an
/// already appended user `type = "group"` entry.
pub(super) fn group_is_defined(keybinds: &Keybinds, id: &str) -> bool {
    is_builtin_group(id) || keybinds.groups.iter().any(|group| group.id == id)
}

/// Direct member keys parsed without the prefix registry: a menu's keys only
/// have to be unique within the menu.
pub(super) fn parse_group_member_bindings(
    field: &str,
    config: &super::BindingConfig,
    diagnostics: &mut Vec<String>,
) -> ActionKeybinds {
    let mut bindings = Vec::new();
    for raw in config.values() {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let Some(combo) = parse_key_combo(raw) else {
            let diag = format!("invalid keybinding: {field} = {raw:?}; disabling binding");
            warn!(message = %diag, "config diagnostic");
            diagnostics.push(diag);
            continue;
        };
        bindings.push(ResolvedBinding {
            trigger: BindingTrigger::Direct(combo),
            label: format_key_combo(combo),
        });
    }
    ActionKeybinds { bindings }
}

/// Projects the menus recorded by [`add_group`] and the parsed menu
/// `entries` into the final menu list. Built-in menus come first, then user
/// menus in config order. In each menu, user entries come first, then the
/// built-in entries and bundled plugin entries whose key no user entry took.
pub(super) fn resolve_groups(keybinds: &mut Keybinds, entries: Vec<MenuEntry>) {
    let mut groups = std::mem::take(&mut keybinds.groups);
    groups.sort_by_key(|group| builtin_group_position(&group.id).unwrap_or(usize::MAX));
    let user_member_keys: std::collections::HashSet<(&str, super::KeyCombo)> = entries
        .iter()
        .filter(|entry| entry.source == BindingSource::User)
        .flat_map(|entry| {
            entry.keys.bindings.iter().map(move |binding| {
                (
                    entry.group.as_str(),
                    normalize_key_combo(binding.trigger.combo()),
                )
            })
        })
        .collect();
    let bundled = crate::builtin_plugin_assets::default_group_members()
        .iter()
        .filter_map(|&(group, key, action, description)| {
            let key = parse_key_combo(key)?;
            let label = format_key_combo(key);
            Some(MenuEntry {
                group: group.to_owned(),
                keys: ActionKeybinds {
                    bindings: vec![ResolvedBinding {
                        trigger: BindingTrigger::Direct(key),
                        label,
                    }],
                },
                action: KeyGroupAction::PluginAction(action),
                description: Cow::Borrowed(description),
                source: BindingSource::Default,
            })
        })
        .collect::<Vec<_>>();
    let overridden = |entry: &MenuEntry| {
        entry.source == BindingSource::Default
            && entry.keys.bindings.iter().any(|binding| {
                user_member_keys.contains(&(
                    entry.group.as_str(),
                    normalize_key_combo(binding.trigger.combo()),
                ))
            })
    };
    let ordered = entries
        .iter()
        .filter(|entry| entry.source == BindingSource::User)
        .chain(
            entries
                .iter()
                .filter(|entry| entry.source == BindingSource::Default),
        )
        .chain(bundled.iter())
        .filter(|entry| !overridden(entry));
    for entry in ordered {
        let Some(group) = groups.iter_mut().find(|group| group.id == entry.group) else {
            continue;
        };
        let Some(label) = entry.keys.label() else {
            continue;
        };
        group.members.push(KeyGroupMember {
            keys: entry.keys.clone(),
            label,
            action: entry.action,
            description: entry.description.clone(),
        });
    }
    groups.retain(|group| group.user_defined || !group.members.is_empty());
    keybinds.groups = groups;
}

impl Keybinds {
    /// Drops every custom command together with the menu entries that ran
    /// one. A user menu left with no entries goes too; built-in actions stay.
    pub(crate) fn clear_custom_commands(&mut self) {
        self.custom_commands.clear();
        for group in &mut self.groups {
            group
                .members
                .retain(|member| !matches!(member.action, KeyGroupAction::Command(_)));
        }
        self.groups.retain(|group| {
            !group.user_defined || is_builtin_group(&group.id) || !group.members.is_empty()
        });
    }
}

/// A default menu opener displaced by a user binding would silently make a
/// whole menu unreachable, so report it even though a single displaced default
/// action is not reported.
pub(super) fn report_shadowed_menu_openers(
    config: &Config,
    keybinds: &Keybinds,
    registry: &BindingRegistry,
    diagnostics: &mut Vec<String>,
) {
    for (field, configured, effective) in [
        (
            "workspace_menu",
            &config.keys.workspace_menu,
            &keybinds.workspace_menu,
        ),
        ("tab_menu", &config.keys.tab_menu, &keybinds.tab_menu),
        ("pane_menu", &config.keys.pane_menu, &keybinds.pane_menu),
        ("agent_menu", &config.keys.agent_menu, &keybinds.agent_menu),
        ("git_menu", &config.keys.git_menu, &keybinds.git_menu),
        (
            "system_menu",
            &config.keys.system_menu,
            &keybinds.system_menu,
        ),
        ("tui_menu", &config.keys.tui_menu, &keybinds.tui_menu),
    ] {
        if config.keys.key_field_is_user_configured(field) || !effective.bindings.is_empty() {
            continue;
        }
        for raw in configured.values() {
            let Some(ParsedBinding::Single(binding)) = parse_binding_string(raw) else {
                continue;
            };
            let Some(owner) = registry.conflict(&binding) else {
                continue;
            };
            if owner.source != BindingSource::User {
                continue;
            }
            let diag = format!(
                "{}: kept {}, disabled keys.{field}; set keys.{field} to reach this menu",
                binding.label, owner.field
            );
            warn!(message = %diag, "config diagnostic");
            diagnostics.push(diag);
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyModifiers};

    use super::*;
    use crate::config::keybinds::CustomCommandAction;

    /// Renders the resolved menus plus the which-key and help projections.
    /// Bundled plugin members are left out: they belong to the plugin table,
    /// not the core keymap.
    fn menu_snapshot(keybinds: &Keybinds) -> String {
        use std::fmt::Write;
        let plugin_members = keybinds
            .groups
            .iter()
            .flat_map(|group| group.members.iter())
            .filter(|member| matches!(member.action, KeyGroupAction::PluginAction(_)))
            .map(|member| (member.label.clone(), member.description.to_string()))
            .collect::<Vec<_>>();
        let is_plugin_entry = |label: &str, description: &str| {
            plugin_members
                .iter()
                .any(|(l, d)| l == label && d == description)
        };
        let mut out = String::new();
        for group in &keybinds.groups {
            let _ = writeln!(
                out,
                "group {} {:?} opener={:?} user={}",
                group.id,
                group.description,
                group.opener.labels(),
                group.user_defined
            );
            for member in &group.members {
                if matches!(member.action, KeyGroupAction::PluginAction(_)) {
                    continue;
                }
                let _ = writeln!(
                    out,
                    "  {} {:?} {:?} {:?}",
                    member.label,
                    member.keys.labels(),
                    member.action,
                    member.description
                );
            }
        }
        for (key, description) in crate::input::prefix_menu_entries(keybinds) {
            let _ = writeln!(out, "prefix {key} {description}");
        }
        for (title, entries) in crate::input::keybind_help_groups(
            keybinds,
            (KeyCode::Char('b'), KeyModifiers::CONTROL),
            |_| true,
        ) {
            let _ = writeln!(out, "help {title}");
            for (key, description) in entries {
                if !is_plugin_entry(&key, &description) {
                    let _ = writeln!(out, "  {key} {description}");
                }
            }
        }
        out
    }

    #[test]
    fn default_menus_characterization() {
        let actual = menu_snapshot(&Config::default().keybinds());
        let expected = include_str!("menus_default_snapshot.txt");
        assert_eq!(actual, expected, "\n{actual}");
    }

    #[test]
    fn user_menus_characterization() {
        let mut config: Config = toml::from_str(
            r#"
[keys]
new_tab = "prefix+c"

[[keys.command]]
key = "prefix+m"
type = "group"
group = "scripts"
description = "scripts"

[[keys.command]]
key = "b"
group = "scripts"
command = "make build"
description = "build"

[[keys.command]]
key = "prefix+shift+t"
type = "group"
group = "tab"

[[keys.command]]
key = "n"
group = "tab"
type = "pane"
command = "echo custom"
description = "custom new tab"

[[keys.command]]
key = "prefix+e"
type = "group"
group = "empty"

[[keys.command]]
key = "prefix+alt+g"
command = "lazygit"
"#,
        )
        .unwrap();
        config.tuis = vec![crate::config::TuiConfig {
            id: "yazi".into(),
            key: "f".into(),
            title: "yazi".into(),
            description: None,
            command: vec!["yazi".into()],
            platforms: None,
            kind: crate::config::TuiKind::Popup,
            width: None,
            height: None,
        }];
        let actual = menu_snapshot(&config.keybinds());
        let expected = include_str!("menus_user_snapshot.txt");
        assert_eq!(actual, expected, "\n{actual}");
    }

    #[test]
    fn every_action_is_reachable_by_default() {
        use crate::input::KeybindAction as A;

        let keybinds = Keybinds::default();
        let flat = crate::input::KeybindAction::flat_defaults_for_test(&keybinds);
        let in_menu = |action: A| {
            keybinds.groups.iter().any(|group| {
                !group.opener.bindings.is_empty()
                    && group
                        .members
                        .iter()
                        .any(|member| member.action == KeyGroupAction::Builtin(action))
            })
        };
        for (bound, action) in flat {
            // Resize directions are reached through resize mode.
            if matches!(
                action,
                A::ResizePaneLeft | A::ResizePaneDown | A::ResizePaneUp | A::ResizePaneRight
            ) {
                continue;
            }
            assert!(
                bound || in_menu(action),
                "{action:?} has no default top-level key and no menu entry"
            );
        }
    }

    #[test]
    fn default_menus_have_unique_member_keys() {
        for group in Keybinds::default().groups {
            let mut seen = std::collections::HashSet::new();
            for member in &group.members {
                for binding in &member.keys.bindings {
                    assert!(
                        seen.insert(normalize_key_combo(binding.trigger.combo())),
                        "{} menu: duplicate key {}",
                        group.id,
                        member.label
                    );
                }
            }
        }
    }

    #[test]
    fn empty_tui_menu_is_hidden_until_a_tui_is_configured() {
        let keybinds = Config::default().keybinds();
        assert!(!keybinds.groups.iter().any(|group| group.id == TUI_GROUP));
        assert!(!keybinds.tui_menu.bindings.is_empty());
    }

    #[test]
    fn bundled_worktrunk_actions_join_the_git_menu() {
        if cfg!(windows) {
            return;
        }
        let git = Config::default()
            .keybinds()
            .groups
            .into_iter()
            .find(|group| group.id == GIT_GROUP)
            .expect("git menu");
        assert!(git.members.iter().any(|member| member.label == "s"
            && member.action == KeyGroupAction::PluginAction("worktrunk.switch")));
    }

    #[test]
    fn configured_tuis_project_popup_and_tab_actions_under_one_group() {
        let mut config = Config::default();
        config.tuis = vec![
            crate::config::TuiConfig {
                id: "git".into(),
                key: "g".into(),
                title: "Git".into(),
                description: None,
                command: vec!["lazygit".into()],
                platforms: None,
                kind: crate::config::TuiKind::Popup,
                width: Some(crate::popup_size::PopupSize::Cells(90)),
                height: None,
            },
            crate::config::TuiConfig {
                id: "nvim".into(),
                key: "n".into(),
                title: "Nvim".into(),
                description: None,
                command: vec!["nvim".into()],
                platforms: None,
                kind: crate::config::TuiKind::Tab,
                width: Some(crate::popup_size::PopupSize::Cells(40)),
                height: Some(crate::popup_size::PopupSize::Cells(20)),
            },
        ];
        let keybinds = config.keybinds();
        let members = keybinds.custom_commands.iter().collect::<Vec<_>>();
        assert_eq!(members.len(), 2);
        let popup = members
            .iter()
            .find(|binding| binding.command == "tui:git")
            .unwrap();
        assert_eq!(popup.argv.as_deref(), Some(&["lazygit".to_owned()][..]));
        assert_eq!(popup.action, CustomCommandAction::Popup);
        assert_eq!(popup.width, Some(crate::popup_size::PopupSize::Cells(90)));
        assert!(popup
            .bindings
            .matches_direct_key(&crate::input::TerminalKey::new(
                KeyCode::Char('g'),
                KeyModifiers::empty()
            )));
        let tab = members
            .iter()
            .find(|binding| binding.command == "tui:nvim")
            .unwrap();
        assert_eq!(tab.action, CustomCommandAction::Tab);
        assert_eq!(tab.width, None);
        assert_eq!(tab.height, None);
        let tab_combo = crate::input::TerminalKey::new(KeyCode::Char('n'), KeyModifiers::empty());
        assert!(tab.bindings.matches_direct_key(&tab_combo));
        assert!(crate::input::resolve_custom_command(
            &keybinds,
            &tab_combo,
            crate::input::KeybindDispatch::Prefix
        )
        .is_none());
        assert_eq!(crate::input::group_entries(&keybinds, TUI_GROUP).len(), 2);
        assert!(keybinds.groups.iter().any(|group| group.id == TUI_GROUP));
    }

    #[test]
    fn grouped_commands_use_bare_member_keys_and_hide_members_from_top_level() {
        let config: Config = toml::from_str(
            r#"
[[keys.command]]
key = "prefix+w"
type = "group"
group = "worktrunk"
description = "worktrunk"

[[keys.command]]
        key = "x"
group = "worktrunk"
command = "worktrunk.switch"
description = "switch"
"#,
        )
        .unwrap();
        let keybinds = config.keybinds();
        assert_eq!(keybinds.custom_commands.len(), 1);
        assert!(keybinds.custom_commands[0].bindings.matches_direct_key(
            &crate::input::TerminalKey::new(KeyCode::Char('x'), KeyModifiers::empty())
        ));
        assert!(crate::input::resolve_custom_command(
            &keybinds,
            &crate::input::TerminalKey::new(KeyCode::Char('x'), KeyModifiers::empty()),
            crate::input::KeybindDispatch::Prefix
        )
        .is_none());
        assert_eq!(crate::input::group_entries(&keybinds, "worktrunk").len(), 1);
        let group = keybinds
            .groups
            .iter()
            .find(|group| group.id == "worktrunk")
            .expect("user menu");
        assert!(group.user_defined);
        assert_eq!(group.description, "worktrunk");
        assert_eq!(group.opener.label().as_deref(), Some("prefix+w"));
    }

    #[test]
    fn grouped_command_validation_rejects_orphans_and_duplicates() {
        let config: Config = toml::from_str(
            r#"
[[keys.command]]
key = "s"
group = "missing"
command = "one"

[[keys.command]]
key = "prefix+w"
type = "group"
group = "outer"

[[keys.command]]
key = "prefix+w"
type = "group"
group = "outer"
"#,
        )
        .unwrap();
        let diagnostics = config.collect_diagnostics();
        assert!(diagnostics
            .iter()
            .any(|d| d.contains("orphan keybind group")));

        let duplicate: Config = toml::from_str(
            r#"
[[keys.command]]
key = "prefix+w"
type = "group"
group = "outer"

[[keys.command]]
key = "s"
group = "outer"
command = "one"

[[keys.command]]
key = "s"
group = "outer"
command = "two"
"#,
        )
        .unwrap();
        assert!(duplicate
            .collect_diagnostics()
            .iter()
            .any(|d| d.contains("duplicate group member key")));
        assert_eq!(
            duplicate
                .keybinds()
                .custom_commands
                .iter()
                .filter(|binding| binding.group.as_deref() == Some("outer"))
                .count(),
            1
        );
    }

    #[test]
    fn group_opener_group_field_never_treated_as_nested_membership() {
        // A `type = "group"` entry's `group` field always names the id it
        // *opens*, never membership in another group, so recursive/nested
        // menus have no config representation regardless of what id is
        // reused here.
        let config: Config = toml::from_str(
            r#"
[[keys.command]]
key = "prefix+w"
type = "group"
group = "outer"

[[keys.command]]
key = "prefix+s"
group = "outer"
type = "group"
"#,
        )
        .unwrap();
        let keybinds = config.keybinds();
        assert_eq!(crate::input::group_entries(&keybinds, "outer").len(), 0);
        assert!(keybinds.custom_commands.is_empty());
        let outer = keybinds
            .groups
            .iter()
            .filter(|group| group.id == "outer")
            .collect::<Vec<_>>();
        assert_eq!(outer.len(), 1);
        assert_eq!(outer[0].opener.bindings.len(), 2);
    }

    #[test]
    fn user_opener_for_a_builtin_id_opens_the_builtin_menu() {
        let config: Config = toml::from_str(
            r#"
[[keys.command]]
key = "prefix+shift+t"
type = "group"
group = "tab"
"#,
        )
        .unwrap();
        let keybinds = config.keybinds();
        let tabs = keybinds
            .groups
            .iter()
            .filter(|group| group.id == TAB_GROUP)
            .collect::<Vec<_>>();
        assert_eq!(tabs.len(), 1);
        assert_eq!(
            tabs[0].opener.bindings.len(),
            Keybinds::default().tab_menu.bindings.len() + 1
        );
        assert!(!tabs[0].members.is_empty());
    }

    #[test]
    fn clearing_custom_commands_drops_user_menus_and_entries() {
        let config: Config = toml::from_str(
            r#"
[[keys.command]]
key = "prefix+w"
type = "group"
group = "worktrunk"

[[keys.command]]
key = "x"
group = "worktrunk"
command = "echo one"

[[keys.command]]
key = "n"
group = "tab"
command = "echo two"
"#,
        )
        .unwrap();
        let mut keybinds = config.keybinds();
        keybinds.clear_custom_commands();
        assert!(!keybinds.groups.iter().any(|group| group.id == "worktrunk"));
        assert!(keybinds.groups.iter().all(|group| group
            .members
            .iter()
            .all(|member| !matches!(member.action, KeyGroupAction::Command(_)))));
    }

    #[test]
    fn default_menu_fragment_parses_without_diagnostics() {
        assert!(!default_menu_entries().is_empty());
        assert!(Config::default().collect_diagnostics().is_empty());
    }

    #[test]
    fn user_menu_with_builtin_members_resolves_and_is_listed_in_help() {
        let config: Config = toml::from_str(
            r#"
[[keys.command]]
key = "prefix+m"
type = "group"
group = "win"
description = "windows"

[[keys.command]]
key = "v"
group = "win"
action = "split_vertical"

[[keys.command]]
key = "s"
group = "win"
action = "split_horizontal"
description = "stack"

[[keys.command]]
key = "n"
group = "tab"
action = "rename_tab"
"#,
        )
        .unwrap();
        assert!(config.collect_diagnostics().is_empty());
        let keybinds = config.keybinds();
        let key = |ch| crate::input::TerminalKey::new(KeyCode::Char(ch), KeyModifiers::empty());
        assert!(matches!(
            crate::input::resolve_group_key(&keybinds, "win", &key('v')),
            Some(crate::input::KeybindMatch::Action(
                crate::input::KeybindAction::SplitVertical
            ))
        ));
        // A user member on a built-in member's key wins.
        assert!(matches!(
            crate::input::resolve_group_key(&keybinds, "tab", &key('n')),
            Some(crate::input::KeybindMatch::Action(
                crate::input::KeybindAction::RenameTab
            ))
        ));
        assert_eq!(
            crate::input::group_entries(&keybinds, "tab")
                .iter()
                .filter(|(key, _)| key == "n")
                .count(),
            1
        );
        // Built-in action entries are keymap, not commands.
        assert!(keybinds.custom_commands.is_empty());
        let help = crate::input::keybind_help_groups(
            &keybinds,
            (KeyCode::Char('b'), KeyModifiers::CONTROL),
            |_| true,
        );
        let win = help
            .iter()
            .find(|(title, _)| title == "windows menu (prefix+m)")
            .expect("help lists the user menu");
        assert_eq!(
            win.1,
            vec![
                ("v".to_owned(), Cow::Borrowed("split side by side")),
                ("s".to_owned(), Cow::Borrowed("stack")),
            ]
        );
    }

    #[test]
    fn invalid_action_entries_are_diagnosed_and_disabled() {
        let config: Config = toml::from_str(
            r#"
[[keys.command]]
key = "v"
group = "pane"
action = "split_sideways"

[[keys.command]]
key = "prefix+v"
action = "split_vertical"

[[keys.command]]
key = "v"
group = "pane"
action = "split_vertical"
command = "echo"

[[keys.command]]
key = "w"
group = "pane"
type = "plugin_action"
action = "split_vertical"
"#,
        )
        .unwrap();
        let diagnostics = config.collect_diagnostics();
        for expected in [
            "unknown action: keys.command[0].action",
            "built-in action outside a menu: keys.command[1].action",
            "conflicting custom command: keys.command[2]",
            "conflicting custom command: keys.command[3]",
        ] {
            assert!(
                diagnostics.iter().any(|d| d.contains(expected)),
                "missing {expected:?} in {diagnostics:?}"
            );
        }
        let keybinds = config.keybinds();
        let pane = keybinds
            .groups
            .iter()
            .find(|group| group.id == PANE_GROUP)
            .unwrap();
        assert!(!pane.members.iter().any(|member| member.label == "w"));
        assert!(keybinds.custom_commands.is_empty());
    }

    /// The whole default menu layout is expressible as user config: the
    /// built-in fragment, re-keyed under new menu ids, resolves to the same
    /// menus.
    #[test]
    fn user_config_reproduces_the_default_menus() {
        #[derive(serde::Serialize)]
        struct Keys {
            command: Vec<CommandKeybindConfig>,
        }
        #[derive(serde::Serialize)]
        struct Fragment {
            keys: Keys,
        }
        let mut openers = 0;
        let command = default_menu_entries()
            .iter()
            .cloned()
            .map(|mut entry| {
                entry.group = entry.group.map(|id| format!("my_{id}"));
                if entry.action_type == CommandKeybindType::Group {
                    openers += 1;
                    entry.key = super::super::BindingConfig::one(format!("prefix+f{openers}"));
                }
                entry
            })
            .collect();
        let text = toml::to_string(&Fragment {
            keys: Keys { command },
        })
        .unwrap();
        let config: Config = toml::from_str(&text).unwrap();
        assert!(config.collect_diagnostics().is_empty(), "{text}");
        let keybinds = config.keybinds();
        let members = |group: &KeyGroup| {
            group
                .members
                .iter()
                .filter(|member| !matches!(member.action, KeyGroupAction::PluginAction(_)))
                .map(|member| {
                    (
                        member.label.clone(),
                        member.action,
                        member.description.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        for builtin in Keybinds::default().groups {
            let user = keybinds
                .groups
                .iter()
                .find(|group| group.id == format!("my_{}", builtin.id))
                .expect("user copy of the menu");
            assert_eq!(user.description, builtin.description);
            assert_eq!(members(user), members(&builtin), "{}", builtin.id);
        }
    }
}
