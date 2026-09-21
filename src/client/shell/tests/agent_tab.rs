use super::*;

const AGENT_TAB_METHODS: [&str; 2] = ["agent.kinds", "agent.open_tab"];

fn key(code: crossterm::event::KeyCode, modifiers: KeyModifiers) -> crate::input::TerminalKey {
    crate::input::TerminalKey::new(code, modifiers)
}

fn press(state: &mut ClientShellState, key: crate::input::TerminalKey) -> ClientShellInput {
    state.handle_raw_events(vec![crate::raw_input::RawInputEvent::Key(key)])
}

fn press_char(state: &mut ClientShellState, ch: char) -> ClientShellInput {
    press(
        state,
        key(crossterm::event::KeyCode::Char(ch), KeyModifiers::empty()),
    )
}

fn open_agent_menu(state: &mut ClientShellState) {
    press(
        state,
        key(crossterm::event::KeyCode::Char('b'), KeyModifiers::CONTROL),
    );
    press_char(state, 'a');
}

fn agent_tab_state(config: Config, methods: Option<&[&str]>) -> ClientShellState {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    state
        .set_endpoint_methods(methods.map(|methods| methods.iter().map(|m| (*m).into()).collect()));
    state
}

fn kinds(names: &[&str]) -> crate::api::schema::ResponseResult {
    crate::api::schema::ResponseResult::AgentKinds {
        kinds: names
            .iter()
            .map(|name| crate::api::schema::AgentKindInfo {
                kind: (*name).into(),
                executable: (*name).into(),
            })
            .collect(),
    }
}

fn single_request(input: &ClientShellInput) -> &crate::api::schema::Request {
    let [ClientShellAction::Endpoint { request, .. }] = &input.actions[..] else {
        panic!("expected exactly one endpoint request: {:?}", input.actions);
    };
    request
}

/// Presses `a n` and answers the `agent.kinds` request with `installed`.
fn open_picker(state: &mut ClientShellState, installed: &[&str]) {
    open_agent_menu(state);
    let prepare = press_char(state, 'n');
    let request = single_request(&prepare);
    assert!(matches!(
        request.method,
        crate::api::schema::Method::AgentKinds(_)
    ));
    let request_id = request.id.clone();
    state.handle_endpoint_result("boot-1", &request_id, Ok(kinds(installed)));
    assert!(matches!(
        state.overlay,
        Some(ClientShellOverlay::AgentTab(_))
    ));
}

fn frame_text(state: &mut ClientShellState) -> String {
    let frame = state.compose(106, 30).expect("agent tab frame");
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
fn agent_menu_offers_new_agent_tab_when_the_runtime_advertises_it() {
    let mut config = Config::default();
    config.keys.show_which_key = true;
    let mut state = agent_tab_state(config, Some(&AGENT_TAB_METHODS));

    open_agent_menu(&mut state);

    let hint = state.key_hint.as_ref().expect("agent menu hint");
    let keys = hint
        .bindings
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    assert_eq!(keys, ["n", "a", "j", "k"]);
    assert!(hint
        .bindings
        .iter()
        .any(|(key, label)| key == "n" && label == "new agent tab…"));
}

#[test]
fn agent_menu_hides_new_agent_tab_on_runtimes_without_it() {
    let mut config = Config::default();
    config.keys.show_which_key = true;
    let mut state = agent_tab_state(config, Some(&["pane.focus"]));

    open_agent_menu(&mut state);
    let keys = state
        .key_hint
        .as_ref()
        .expect("agent menu hint")
        .bindings
        .iter()
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    assert_eq!(keys, ["a", "j", "k"]);
    let pressed = press_char(&mut state, 'n');
    assert!(pressed.actions.is_empty());
    assert!(state.overlay.is_none());
    assert!(state.visible_endpoint_notice.is_none());

    // The rest of the agent menu still resolves.
    open_agent_menu(&mut state);
    let next = press_char(&mut state, 'j');
    assert!(matches!(
        &next.actions[..],
        [ClientShellAction::Keybind(
            crate::input::KeybindAction::NextAgent
        )]
    ));
}

#[test]
fn agent_picker_requests_installed_agents_once_per_open() {
    let mut state = agent_tab_state(Config::default(), Some(&AGENT_TAB_METHODS));
    open_picker(&mut state, &["claude"]);
    press(
        &mut state,
        key(crossterm::event::KeyCode::Esc, KeyModifiers::empty()),
    );
    open_picker(&mut state, &["claude", "codex"]);

    let Some(ClientShellOverlay::AgentTab(picker)) = &state.overlay else {
        panic!("picker should be open");
    };
    assert_eq!(picker.entries.len(), 2);
}

#[test]
fn agent_picker_filters_by_typing_and_opens_the_highlighted_agent() {
    let mut config = Config::default();
    config.ui.prompt_new_tab_name = true;
    let mut state = agent_tab_state(config, Some(&AGENT_TAB_METHODS));
    open_picker(&mut state, &["claude", "codex", "opencode"]);

    let text = frame_text(&mut state);
    assert!(text.contains("new agent tab"));
    assert!(text.contains("3 agents"));

    press_char(&mut state, 'c');
    press_char(&mut state, 'o');
    let text = frame_text(&mut state);
    assert!(text.contains("2/3 agents"), "{text}");
    assert!(!text.contains("claude"), "{text}");
    press(
        &mut state,
        key(crossterm::event::KeyCode::Down, KeyModifiers::empty()),
    );

    let open = press(
        &mut state,
        key(crossterm::event::KeyCode::Enter, KeyModifiers::empty()),
    );
    // Opening an agent tab never goes through the new-tab name prompt.
    assert!(matches!(
        &single_request(&open).method,
        crate::api::schema::Method::AgentOpenTab(params) if params.kind == "opencode"
    ));
    let Some(ClientShellOverlay::AgentTab(picker)) = &state.overlay else {
        panic!("picker stays open until the runtime answers");
    };
    assert!(picker.opening);
}

#[test]
fn agent_picker_escape_cancels_without_starting_anything() {
    let mut state = agent_tab_state(Config::default(), Some(&AGENT_TAB_METHODS));
    open_picker(&mut state, &["claude"]);

    let cancel = press(
        &mut state,
        key(crossterm::event::KeyCode::Esc, KeyModifiers::empty()),
    );

    assert!(cancel.actions.is_empty());
    assert!(state.overlay.is_none());
    assert_eq!(state.mode, ClientShellMode::Terminal);
}

#[test]
fn agent_picker_states_when_no_supported_agent_is_installed() {
    let mut state = agent_tab_state(Config::default(), Some(&AGENT_TAB_METHODS));
    open_picker(&mut state, &[]);

    let text = frame_text(&mut state);
    assert!(
        text.contains("no supported agent found on the runtime host's PATH"),
        "{text}"
    );
    let enter = press(
        &mut state,
        key(crossterm::event::KeyCode::Enter, KeyModifiers::empty()),
    );
    assert!(enter.actions.is_empty());
}

#[test]
fn agent_tab_failure_is_a_notice_that_keeps_the_picker_usable() {
    let mut state = agent_tab_state(Config::default(), Some(&AGENT_TAB_METHODS));
    open_picker(&mut state, &["claude"]);
    let open = press(
        &mut state,
        key(crossterm::event::KeyCode::Enter, KeyModifiers::empty()),
    );
    let request_id = single_request(&open).id.clone();

    let (_, actions) = state.handle_endpoint_result(
        "boot-1",
        &request_id,
        Err(ClientShellEndpointError {
            code: Some("agent_not_installed".into()),
            message: "claude was not found on the runtime host's PATH".into(),
        }),
    );

    assert!(actions.is_empty(), "failure must not retry automatically");
    assert!(state.visible_endpoint_notice.is_some());
    let Some(ClientShellOverlay::AgentTab(picker)) = &state.overlay else {
        panic!("failed open should keep the picker");
    };
    assert!(!picker.opening);
    assert_eq!(
        picker.error.as_deref(),
        Some("claude was not found on the runtime host's PATH")
    );
    // The session stays connected and the keymap keeps working.
    assert!(state.endpoint_is_online(&state.active_endpoint_id.clone()));
    press(
        &mut state,
        key(crossterm::event::KeyCode::Esc, KeyModifiers::empty()),
    );
    open_agent_menu(&mut state);
    let next = press_char(&mut state, 'j');
    assert!(matches!(
        &next.actions[..],
        [ClientShellAction::Keybind(
            crate::input::KeybindAction::NextAgent
        )]
    ));
}
