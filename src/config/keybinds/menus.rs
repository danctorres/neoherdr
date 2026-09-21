//! Which-key menus.
//!
//! Built-in menus (`workspace`, `tab`, ...), user menus opened by
//! `[[keys.command]] type = "group"` entries, and the entries that join a
//! menu through `group = "<id>"` are all projected into one resolved
//! [`KeyGroup`] list on [`Keybinds::groups`]. Input resolution, the which-key
//! hint, and the keybind help read only that list.

use std::borrow::Cow;

use tracing::warn;

use super::{
    format_key_combo, normalize_key_combo, parse_binding_string, parse_key_combo, ActionKeybinds,
    BindingRegistry, BindingSource, BindingTrigger, KeyCombo, Keybinds, ParsedBinding,
    ResolvedBinding,
};
use crate::config::Config;

pub(crate) const WORKSPACE_GROUP: &str = "workspace";
pub(crate) const TAB_GROUP: &str = "tab";
pub(crate) const PANE_GROUP: &str = "pane";
pub(crate) const AGENT_GROUP: &str = "agent";
pub(crate) const GIT_GROUP: &str = "git";
pub(crate) const SYSTEM_GROUP: &str = "system";
/// Menu holding the TUIs configured in tuis.toml.
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
    /// it has no entries, and goes away with the custom commands.
    pub(crate) user_defined: bool,
}

struct GroupSpec {
    id: &'static str,
    description: &'static str,
    members: &'static [(&'static str, KeyGroupAction, &'static str)],
}

/// The built-in which-key menus. Menus are named after the object they act
/// on and share one verb vocabulary, so the same key means the same thing in
/// every menu:
///
/// - `n` new, `r` rename, `x` close/remove, `s` switch/pick
/// - direction keys walk the item's own axis: `h`/`l` for the horizontal tab
///   bar, `j`/`k` for the vertical workspace and agent lists
/// - shift turns "go to" into "move": `H`/`L` move a tab
/// - agents get no per-agent keys: `n` in the agent menu opens a picker of
///   the agents installed on the runtime host, so no default key depends on
///   what is installed
fn builtin_group_specs() -> [GroupSpec; 7] {
    use crate::input::KeybindAction as A;
    use KeyGroupAction::Builtin as B;
    [
        GroupSpec {
            id: WORKSPACE_GROUP,
            description: "workspace",
            members: &[
                ("n", B(A::NewWorkspace), "new workspace"),
                ("r", B(A::RenameWorkspace), "rename workspace"),
                ("x", B(A::CloseWorkspace), "close workspace"),
                ("s", B(A::WorkspacePicker), "switch workspace"),
                ("j", B(A::NextWorkspace), "next workspace"),
                ("k", B(A::PreviousWorkspace), "previous workspace"),
                ("g", B(A::ToggleGroup), "expand/collapse group"),
            ],
        },
        GroupSpec {
            id: TAB_GROUP,
            description: "tab",
            members: &[
                ("n", B(A::NewTab), "new tab"),
                ("r", B(A::RenameTab), "rename tab"),
                ("x", B(A::CloseTab), "close tab"),
                ("h", B(A::PreviousTab), "previous tab"),
                ("l", B(A::NextTab), "next tab"),
                ("H", B(A::MoveTabPrevious), "move tab left"),
                ("L", B(A::MoveTabNext), "move tab right"),
            ],
        },
        GroupSpec {
            id: PANE_GROUP,
            description: "pane",
            members: &[
                ("r", B(A::RenamePane), "rename pane"),
                ("c", B(A::ClearPaneName), "clear pane name"),
                ("x", B(A::ClosePane), "close pane"),
                ("z", B(A::Zoom), "zoom pane"),
                ("backslash", B(A::SplitVertical), "split side by side"),
                ("minus", B(A::SplitHorizontal), "split stacked"),
                ("s", B(A::SwapWithFocusedPane), "swap with focused pane"),
                ("p", B(A::LastPane), "previous (last) pane"),
                ("e", B(A::EditScrollback), "edit scrollback"),
                // `k` follows the terminal convention for clearing (cmd+k, ctrl+k).
                ("k", B(A::ClearPane), "clear screen and scrollback"),
                ("y", B(A::CopyMode), "copy mode"),
            ],
        },
        GroupSpec {
            id: AGENT_GROUP,
            description: "agent",
            members: &[
                ("n", B(A::NewAgentTab), "new agent tab…"),
                ("a", B(A::OpenNotificationTarget), "jump to notification"),
                ("j", B(A::NextAgent), "next agent"),
                ("k", B(A::PreviousAgent), "previous agent"),
            ],
        },
        GroupSpec {
            id: GIT_GROUP,
            description: "git",
            members: &[
                ("n", B(A::NewWorktree), "new worktree"),
                ("o", B(A::OpenWorktree), "open worktree"),
                ("x", B(A::RemoveWorktree), "remove worktree"),
            ],
        },
        GroupSpec {
            id: SYSTEM_GROUP,
            description: "system",
            members: &[
                ("s", B(A::Settings), "settings"),
                ("r", B(A::ReloadConfig), "reload config"),
                ("b", B(A::ToggleSidebar), "toggle sidebar"),
                ("?", B(A::Help), "keybinds"),
                ("q", B(A::Detach), "detach"),
            ],
        },
        GroupSpec {
            id: TUI_GROUP,
            description: "open TUI",
            members: &[],
        },
    ]
}

fn group_opener<'a>(keybinds: &'a Keybinds, id: &str) -> &'a ActionKeybinds {
    match id {
        WORKSPACE_GROUP => &keybinds.workspace_menu,
        TAB_GROUP => &keybinds.tab_menu,
        PANE_GROUP => &keybinds.pane_menu,
        AGENT_GROUP => &keybinds.agent_menu,
        GIT_GROUP => &keybinds.git_menu,
        SYSTEM_GROUP => &keybinds.system_menu,
        _ => &keybinds.tui_menu,
    }
}

pub(super) fn is_builtin_group(id: &str) -> bool {
    builtin_group_specs().iter().any(|spec| spec.id == id)
}

/// Records the menu opened by a user `type = "group"` entry. Openers sharing
/// an id open the same menu.
pub(super) fn add_user_group(
    keybinds: &mut Keybinds,
    id: &str,
    description: Option<&str>,
    opener: ActionKeybinds,
) {
    if let Some(group) = keybinds.groups.iter_mut().find(|group| group.id == id) {
        group.opener.bindings.extend(opener.bindings);
        return;
    }
    keybinds.groups.push(KeyGroup {
        id: id.to_owned(),
        description: description.unwrap_or(id).to_owned(),
        opener,
        members: Vec::new(),
        user_defined: true,
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

/// Projects the built-in menus, the user menus recorded by
/// [`add_user_group`], and every grouped custom command into the final menu
/// list. Built-in menus come first, then user menus in config order.
pub(super) fn resolve_groups(keybinds: &mut Keybinds) {
    let user_groups = std::mem::take(&mut keybinds.groups);
    let user_member_keys: std::collections::HashSet<(&str, KeyCombo)> = keybinds
        .custom_commands
        .iter()
        .filter_map(|binding| Some((binding.group.as_deref()?, &binding.bindings)))
        .flat_map(|(group, bindings)| {
            bindings
                .bindings
                .iter()
                .map(move |binding| (group, normalize_key_combo(binding.trigger.combo())))
        })
        .collect();
    let bundled = crate::builtin_plugin_assets::default_group_members();
    let mut groups: Vec<KeyGroup> = builtin_group_specs()
        .into_iter()
        .map(|spec| {
            let members = spec
                .members
                .iter()
                .copied()
                .chain(bundled.iter().filter(|member| member.0 == spec.id).map(
                    |&(_, key, action, description)| {
                        (key, KeyGroupAction::PluginAction(action), description)
                    },
                ))
                .filter_map(|(key, action, description)| {
                    let key = parse_key_combo(key)?;
                    let label = format_key_combo(key);
                    (!user_member_keys.contains(&(spec.id, key))).then(|| KeyGroupMember {
                        keys: ActionKeybinds {
                            bindings: vec![ResolvedBinding {
                                trigger: BindingTrigger::Direct(key),
                                label: label.clone(),
                            }],
                        },
                        label,
                        action,
                        description: Cow::Borrowed(description),
                    })
                })
                .collect();
            KeyGroup {
                id: spec.id.to_owned(),
                description: spec.description.to_owned(),
                opener: group_opener(keybinds, spec.id).clone(),
                members,
                user_defined: false,
            }
        })
        .collect();
    for user in user_groups {
        match groups.iter_mut().find(|group| group.id == user.id) {
            Some(builtin) => {
                builtin.opener.bindings.extend(user.opener.bindings);
                builtin.user_defined = true;
            }
            None => groups.push(user),
        }
    }
    for group in &mut groups {
        let commands = keybinds
            .custom_commands
            .iter()
            .enumerate()
            .filter(|(_, binding)| binding.group.as_deref() == Some(group.id.as_str()))
            .filter_map(|(index, binding)| {
                Some(KeyGroupMember {
                    label: binding.bindings.label()?,
                    keys: binding.bindings.clone(),
                    action: KeyGroupAction::Command(index),
                    description: binding
                        .description
                        .clone()
                        .map(Cow::Owned)
                        .unwrap_or(Cow::Borrowed("custom command")),
                })
            })
            .collect::<Vec<_>>();
        group.members.splice(0..0, commands);
    }
    groups.retain(|group| group.user_defined || !group.members.is_empty());
    keybinds.groups = groups;
}

impl Keybinds {
    /// Drops every custom command together with the menu entries and user
    /// menus that came from them, leaving the built-in menus.
    pub(crate) fn clear_custom_commands(&mut self) {
        self.custom_commands.clear();
        self.groups
            .retain(|group| !group.user_defined || is_builtin_group(&group.id));
        for group in &mut self.groups {
            group
                .members
                .retain(|member| !matches!(member.action, KeyGroupAction::Command(_)));
        }
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
}
