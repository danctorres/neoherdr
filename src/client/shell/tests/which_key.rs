use super::*;

fn prefix_key() -> crate::input::TerminalKey {
    crate::input::TerminalKey::new(
        crossterm::event::KeyCode::Char('b'),
        crossterm::event::KeyModifiers::CONTROL,
    )
}

fn plain_key(code: crossterm::event::KeyCode) -> crate::input::TerminalKey {
    crate::input::TerminalKey::new(code, crossterm::event::KeyModifiers::empty())
}

fn state_with_which_key(show: bool) -> ClientShellState {
    let mut config = Config::default();
    config.keys.show_which_key = show;
    ClientShellState::new(ClientShellConfig::from_config(&config))
}

fn state_with_worktrunk_group() -> ClientShellState {
    let mut config = Config::default();
    config.keys.show_which_key = true;
    config.keys.command = vec![
        crate::config::CommandKeybindConfig {
            key: crate::config::BindingConfig::one("prefix+shift+e"),
            command: String::new(),
            action_type: crate::config::CommandKeybindType::Group,
            description: Some("worktrunk".into()),
            width: None,
            height: None,
            group: Some("worktrunk".into()),
            ..Default::default()
        },
        crate::config::CommandKeybindConfig {
            key: crate::config::BindingConfig::one("s"),
            command: "worktrunk.switch".into(),
            action_type: crate::config::CommandKeybindType::PluginAction,
            description: Some("switch".into()),
            width: None,
            height: None,
            group: Some("worktrunk".into()),
            ..Default::default()
        },
    ];
    ClientShellState::new(ClientShellConfig::from_config(&config))
}

fn state_with_tui() -> ClientShellState {
    let mut config = Config::default();
    config.keys.show_which_key = true;
    config.tuis.push(crate::config::TuiConfig {
        id: "git".into(),
        key: "g".into(),
        title: "Git UI".into(),
        description: Some("open git UI".into()),
        command: vec!["lazygit".into()],
        platforms: None,
        kind: crate::config::TuiKind::Popup,
        width: None,
        height: None,
    });
    ClientShellState::new(ClientShellConfig::from_config(&config))
}

fn state_with_yazi_tui() -> ClientShellState {
    let mut config = Config::default();
    config.keys.show_which_key = true;
    config.tuis.push(crate::config::TuiConfig {
        id: "yazi".into(),
        key: "f".into(),
        title: "yazi".into(),
        description: Some("open yazi".into()),
        command: vec!["yazi".into()],
        platforms: None,
        kind: crate::config::TuiKind::Popup,
        width: None,
        height: None,
    });
    ClientShellState::new(ClientShellConfig::from_config(&config))
}

fn press(state: &mut ClientShellState, key: crate::input::TerminalKey) -> ClientShellInput {
    state.handle_raw_events(vec![crate::raw_input::RawInputEvent::Key(key)])
}

fn frame_text(frame: &crate::protocol::FrameData) -> String {
    frame
        .cells
        .chunks(frame.width as usize)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn key_hint_defaults_to_none() {
    let state = state_with_which_key(true);
    assert!(state.key_hint.is_none());
}

#[test]
fn entering_prefix_populates_hint_when_enabled() {
    let mut state = state_with_which_key(true);
    press(&mut state, prefix_key());

    assert_eq!(state.mode, ClientShellMode::Prefix);
    let hint = state.key_hint.as_ref().expect("which-key hint");
    assert!(hint.visible);
    assert!(!hint.bindings.is_empty());
    // Prefix-reachable bindings are listed by their post-prefix key; menus
    // are marked with a leading `+`.
    for (key, label) in [
        ("w", "+workspace"),
        ("t", "+tab"),
        ("p", "+pane"),
        ("a", "+agent"),
        ("g", "+git"),
        ("s", "+system"),
        ("h/j/k/l", "focus pane"),
        ("shift+h/j/k/l", "swap pane"),
        ("[/]", "previous / next tab"),
        ("tab / shift+tab", "cycle pane"),
        ("n", "new tab"),
        ("?", "keybinds"),
    ] {
        assert!(
            hint.bindings.iter().any(|(k, l)| k == key && l == label),
            "missing {key} {label}: {:?}",
            hint.bindings
        );
    }
    // Menu members are only listed inside their menu.
    assert!(!hint
        .bindings
        .iter()
        .any(|(_, label)| label == "clear pane name"));
    // Keys are unique at the top level.
    let mut keys = std::collections::HashSet::new();
    assert!(hint
        .bindings
        .iter()
        .all(|(key, _)| keys.insert(key.clone())));
    assert!(
        hint.bindings
            .iter()
            .all(|(key, _)| !key.starts_with("prefix+")),
        "hint keys must be post-prefix: {:?}",
        hint.bindings
    );
}

#[test]
fn tui_binding_is_discoverable_in_prefix_hint() {
    let mut state = state_with_tui();
    assert!(!state.config.keybinds.keybinds.custom_commands.is_empty());
    press(&mut state, prefix_key());

    assert!(state
        .key_hint
        .as_ref()
        .expect("which-key hint")
        .bindings
        .iter()
        .any(|(key, label)| key == "o" && label == "+open TUI"));
    press(&mut state, plain_key(crossterm::event::KeyCode::Char('o')));
    assert_eq!(
        state.key_hint.expect("TUI submenu").bindings,
        vec![(
            "g".into(),
            std::borrow::Cow::Borrowed("Git UI: open git UI")
        )]
    );
}

#[test]
fn tui_key_is_local_to_its_menu() {
    // A TUI on `f` lives inside the `open` menu and never shadows a
    // top-level key or another menu's entries.
    let mut state = state_with_yazi_tui();
    press(&mut state, prefix_key());
    press(&mut state, plain_key(crossterm::event::KeyCode::Char('o')));

    assert_eq!(state.mode, ClientShellMode::Prefix);
    assert_eq!(state.active_key_group.as_deref(), Some("open"));
    assert_eq!(
        state.key_hint.expect("TUI submenu").bindings,
        vec![("f".into(), std::borrow::Cow::Borrowed("yazi: open yazi"))]
    );
}

#[test]
fn snapshot_keeps_configured_tui_submenu() {
    let mut state = state_with_tui();
    state.set_snapshot(Box::new(snapshot()));
    press(&mut state, prefix_key());
    assert!(state
        .key_hint
        .as_ref()
        .expect("which-key hint")
        .bindings
        .iter()
        .any(|(key, label)| key == "o" && label == "+open TUI"));
}

#[test]
fn default_git_menu_lists_worktree_actions_with_shared_verbs() {
    let mut state = state_with_which_key(true);
    press(&mut state, prefix_key());
    press(&mut state, plain_key(crossterm::event::KeyCode::Char('g')));

    let submenu = state.key_hint.expect("git submenu").bindings;
    for (key, label) in [
        ("n", "new worktree"),
        ("o", "open worktree"),
        ("x", "remove worktree"),
    ] {
        assert!(
            submenu.iter().any(|(k, l)| k == key && l == label),
            "missing {key} {label}: {submenu:?}"
        );
    }
}

#[test]
fn escape_in_a_menu_returns_to_the_top_level_menu() {
    let mut state = state_with_which_key(true);
    press(&mut state, prefix_key());
    press(&mut state, plain_key(crossterm::event::KeyCode::Char('t')));
    assert_eq!(state.active_key_group.as_deref(), Some("tab"));

    press(&mut state, plain_key(crossterm::event::KeyCode::Esc));
    assert_eq!(state.mode, ClientShellMode::Prefix);
    assert_eq!(state.active_key_group, None);
    assert!(state
        .key_hint
        .as_ref()
        .expect("top-level which-key")
        .bindings
        .iter()
        .any(|(key, label)| key == "t" && label == "+tab"));
}

#[test]
fn entering_prefix_skips_hint_when_disabled() {
    let mut state = state_with_which_key(false);
    press(&mut state, prefix_key());

    assert_eq!(state.mode, ClientShellMode::Prefix);
    assert!(state.key_hint.is_none());
}

#[test]
fn user_menu_of_builtin_actions_shows_in_which_key_and_dispatches() {
    let config: Config = toml::from_str(
        r#"
[keys]
show_which_key = true

[[keys.command]]
key = "prefix+m"
type = "group"
group = "win"
description = "windows"

[[keys.command]]
key = "v"
group = "win"
action = "split_vertical"
"#,
    )
    .unwrap();
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
    // Local keybindings are rebuilt from the snapshot; built-in action entries
    // are client keymap and must survive without a matching server command.
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());

    press(&mut state, prefix_key());
    assert!(state
        .key_hint
        .as_ref()
        .expect("top-level which-key")
        .bindings
        .iter()
        .any(|(key, label)| key == "m" && label == "+windows"));
    press(&mut state, plain_key(crossterm::event::KeyCode::Char('m')));
    let menu = state.key_hint.as_ref().expect("win menu");
    assert_eq!(
        menu.bindings,
        vec![("v".into(), std::borrow::Cow::Borrowed("split side by side"))]
    );
    let outcome = press(&mut state, plain_key(crossterm::event::KeyCode::Char('v')));
    assert!(
        !outcome.actions.is_empty(),
        "expected split action, got {:?}",
        outcome.actions
    );
}

#[test]
fn group_opener_replaces_which_key_with_its_submenu() {
    let mut state = state_with_worktrunk_group();
    press(&mut state, prefix_key());

    let top_level = state.key_hint.as_ref().expect("top-level which-key");
    assert!(top_level
        .bindings
        .iter()
        .any(|(key, label)| key == "shift+e" && label == "+worktrunk"));
    assert!(!top_level
        .bindings
        .iter()
        .any(|(_, label)| label == "switch" || label == "list"));

    press(
        &mut state,
        crate::input::TerminalKey::new(
            crossterm::event::KeyCode::Char('E'),
            crossterm::event::KeyModifiers::SHIFT,
        ),
    );

    let submenu = state.key_hint.as_ref().expect("Worktrunk submenu");
    assert_eq!(
        submenu.bindings,
        vec![("s".into(), std::borrow::Cow::Borrowed("switch"))]
    );
}

#[test]
fn leaving_prefix_clears_hint() {
    let mut state = state_with_which_key(true);
    press(&mut state, prefix_key());
    assert!(state.key_hint.is_some());

    press(&mut state, plain_key(crossterm::event::KeyCode::Esc));
    assert_eq!(state.mode, ClientShellMode::Terminal);
    assert!(state.key_hint.is_none());
}

#[test]
fn resolving_prefix_binding_clears_hint() {
    let mut state = state_with_which_key(true);
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    press(&mut state, prefix_key());
    assert!(state.key_hint.is_some());

    press(&mut state, plain_key(crossterm::event::KeyCode::Char('t')));
    press(&mut state, plain_key(crossterm::event::KeyCode::Char('r')));
    assert_eq!(state.mode, ClientShellMode::Terminal);
    assert!(state.key_hint.is_none());
    assert!(matches!(state.overlay, Some(ClientShellOverlay::Rename(_))));
}

#[test]
fn prefix_resolution_is_identical_with_hint_on_or_off() {
    // The hint is a side effect of entering Prefix mode, never on the
    // critical path of key resolution: the same keypress must dispatch the
    // same action with the same latency characteristics either way.
    for show in [true, false] {
        let mut state = state_with_which_key(show);
        state.set_snapshot(Box::new(snapshot()));
        state.set_pane_surface(surface());
        press(&mut state, prefix_key());
        press(&mut state, plain_key(crossterm::event::KeyCode::Char('t')));
        press(&mut state, plain_key(crossterm::event::KeyCode::Char('r')));
        assert_eq!(state.mode, ClientShellMode::Terminal);
        assert!(state.key_hint.is_none());
        assert!(matches!(state.overlay, Some(ClientShellOverlay::Rename(_))));
    }
}

#[test]
fn prefix_hint_keeps_one_entry_per_key_when_user_overrides_opener() {
    let mut config = Config::default();
    config.keys.show_which_key = true;
    config.keys.command = vec![crate::config::CommandKeybindConfig {
        key: crate::config::BindingConfig::one("prefix+s"),
        command: "worktrunk.list".into(),
        action_type: crate::config::CommandKeybindType::PluginAction,
        description: Some("open lazygit".into()),
        width: None,
        height: None,
        group: None,
        ..Default::default()
    }];
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
    press(&mut state, prefix_key());
    let hint = state.key_hint.as_ref().expect("which-key hint");
    let s_labels = hint
        .bindings
        .iter()
        .filter(|(key, _)| key == "s")
        .map(|(_, label)| label.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(s_labels, ["open lazygit"]);
}

#[test]
fn key_hint_renders_when_visible_and_not_otherwise() {
    let mut shown = state_with_which_key(true);
    shown.set_snapshot(Box::new(snapshot()));
    shown.set_pane_surface(surface());
    press(&mut shown, prefix_key());
    let frame = shown.compose(100, 30).expect("hint frame");
    let text = frame_text(&frame);
    assert!(text.contains("+pane"));
    // Unbound actions get no status bar hint.
    assert!(!text.contains("unset"), "{text}");

    // A submenu names itself and pads keys so labels share a column.
    press(
        &mut shown,
        crate::input::TerminalKey::new(
            crossterm::event::KeyCode::Char('w'),
            crossterm::event::KeyModifiers::empty(),
        ),
    );
    let frame = shown.compose(100, 30).expect("submenu frame");
    let text = frame_text(&frame);
    assert!(text.contains("workspace · esc back"), "{text}");
    assert!(text.contains("n        new workspace"), "{text}");
    assert!(text.contains("shift+j  move workspace down"), "{text}");

    let mut hidden = state_with_which_key(false);
    hidden.set_snapshot(Box::new(snapshot()));
    hidden.set_pane_surface(surface());
    press(&mut hidden, prefix_key());
    let frame = hidden.compose(100, 30).expect("plain prefix frame");
    assert!(!frame_text(&frame).contains("+pane"));
}

#[test]
fn grouped_tab_command_survives_the_endpoint_manifest_and_resolves_in_its_menu() {
    // The endpoint manifest advertises `type = "tab"` as `Pane`; the local
    // entry must still recover its menu from the matching manifest command.
    let mut config = Config::default();
    config.keys.show_which_key = true;
    config.keys.command = vec![crate::config::CommandKeybindConfig {
        key: crate::config::BindingConfig::one("v"),
        command: "lazygit".into(),
        action_type: crate::config::CommandKeybindType::Tab,
        description: Some("lazygit tab".into()),
        group: Some("git".into()),
        ..Default::default()
    }];
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
    let mut projection = snapshot();
    projection
        .commands
        .push(crate::protocol::ClientShellCommand {
            command_id: "cmd_git_tab".into(),
            binding_label: "v".into(),
            binding_labels: vec!["v".into()],
            action: crate::protocol::ClientShellCommandAction::Pane,
            description: Some("lazygit tab".into()),
        });
    state.set_snapshot(Box::new(projection));
    state.set_pane_surface(surface());

    press(&mut state, prefix_key());
    press(&mut state, plain_key(crossterm::event::KeyCode::Char('g')));
    let menu = state.key_hint.as_ref().expect("git menu");
    assert!(menu
        .bindings
        .iter()
        .any(|(key, label)| key == "v" && label == "lazygit tab"));

    let outcome = press(&mut state, plain_key(crossterm::event::KeyCode::Char('v')));
    let [ClientShellAction::Endpoint { request, .. }] = &outcome.actions[..] else {
        panic!(
            "expected endpoint command invocation, got {:?}",
            outcome.actions
        );
    };
    let crate::api::schema::Method::CommandInvoke(params) = &request.method else {
        panic!("expected command.invoke");
    };
    assert_eq!(params.command_id, "cmd_git_tab");
}

fn worktrunk_plugin(enabled: bool) -> crate::api::schema::InstalledPluginInfo {
    let action = |id: &str| crate::api::schema::PluginManifestAction {
        id: id.into(),
        title: id.into(),
        description: None,
        contexts: vec![],
        platforms: None,
        command: vec!["sh".into()],
    };
    crate::api::schema::InstalledPluginInfo {
        plugin_id: "worktrunk".into(),
        name: "Worktrunk".into(),
        version: "0.1.0".into(),
        min_herdr_version: "0.9.0".into(),
        description: None,
        manifest_path: "/tmp/worktrunk/herdr-plugin.toml".into(),
        plugin_root: "/tmp/worktrunk".into(),
        enabled,
        platforms: None,
        build: vec![],
        startup: vec![],
        actions: ["switch", "list", "remove", "merge"]
            .into_iter()
            .map(action)
            .collect(),
        events: vec![],
        panes: vec![],
        link_handlers: vec![],
        source: Default::default(),
        warnings: vec![],
    }
}

/// Opens the git menu and answers the `plugin.list` request it sends.
fn open_git_menu_with_plugins(
    state: &mut ClientShellState,
    plugins: Vec<crate::api::schema::InstalledPluginInfo>,
) {
    assert!(press(state, prefix_key()).actions.is_empty());
    let input = press(state, plain_key(crossterm::event::KeyCode::Char('g')));
    let [ClientShellAction::Endpoint { request, .. }] = &input.actions[..] else {
        panic!("expected a plugin.list request: {:?}", input.actions);
    };
    assert!(matches!(
        request.method,
        crate::api::schema::Method::PluginList(_)
    ));
    let request_id = request.id.clone();
    state.handle_endpoint_result(
        "boot-1",
        &request_id,
        Ok(crate::api::schema::ResponseResult::PluginList { plugins }),
    );
}

fn menu_labels(state: &ClientShellState) -> Vec<String> {
    state
        .key_hint
        .as_ref()
        .expect("submenu hint")
        .bindings
        .iter()
        .map(|(_, label)| label.to_string())
        .collect()
}

#[cfg(not(windows))]
#[test]
fn bundled_plugin_menu_entries_follow_the_endpoint_plugin_list() {
    let mut state = state_with_which_key(true);
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());

    open_git_menu_with_plugins(&mut state, vec![worktrunk_plugin(true)]);
    assert!(menu_labels(&state)
        .iter()
        .any(|label| label.starts_with("worktrunk")));
    press(&mut state, plain_key(crossterm::event::KeyCode::Esc));
    press(&mut state, plain_key(crossterm::event::KeyCode::Esc));

    for plugins in [vec![worktrunk_plugin(false)], vec![]] {
        open_git_menu_with_plugins(&mut state, plugins);
        let labels = menu_labels(&state);
        assert!(
            !labels.iter().any(|label| label.starts_with("worktrunk")),
            "{labels:?}"
        );
        assert!(labels.iter().any(|label| label == "new worktree"));
        // A hidden entry's key does nothing instead of invoking the action.
        let input = press(&mut state, plain_key(crossterm::event::KeyCode::Char('s')));
        assert!(input.actions.is_empty(), "{:?}", input.actions);
        assert_eq!(state.mode, ClientShellMode::Terminal);
    }
}

#[cfg(not(windows))]
#[test]
fn help_panel_hides_unavailable_bundled_plugin_entries() {
    let mut state = state_with_which_key(true);
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    let help = |state: &ClientShellState| {
        crate::input::keybind_help_groups(
            &state.config.keybinds.keybinds,
            &state.config.keybinds.prefix,
            |action| state.key_group_action_available(action),
        )
        .into_iter()
        .flat_map(|(_, entries)| entries)
        .any(|(_, label)| label.starts_with("worktrunk"))
    };

    assert!(!help(&state));
    open_git_menu_with_plugins(&mut state, vec![worktrunk_plugin(true)]);
    assert!(help(&state));
}
