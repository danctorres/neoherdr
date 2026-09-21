use super::*;

/// One row of a filterable list picker: a bold label with a right-aligned
/// status, and a dimmer detail line below it.
pub(super) struct PickerRow<'a> {
    pub(super) label: &'a str,
    pub(super) status: &'a str,
    pub(super) detail: &'a str,
}

/// What a filterable list picker draws. The worktree-open and agent-tab
/// pickers share this layout so they look and behave alike.
pub(super) struct PickerView<'a> {
    pub(super) title: &'a str,
    /// Shown after ` / ` while the filter is empty and unfocused.
    pub(super) placeholder: &'a str,
    /// Plural noun for the `n items` / `n/m items` count.
    pub(super) count_noun: &'a str,
    /// Shown when no row matches the filter.
    pub(super) empty: &'a str,
    pub(super) confirm: &'a str,
    pub(super) rows: Vec<PickerRow<'a>>,
    pub(super) filtered: &'a [usize],
    pub(super) selected: usize,
    pub(super) query: &'a TextEditor,
    pub(super) search_focused: bool,
    /// Replaces the error line while a confirmed request is in flight.
    pub(super) busy: Option<&'a str>,
    pub(super) error: Option<&'a str>,
}

pub(super) fn render_picker_overlay(
    b: &mut Buffer,
    view: &PickerView<'_>,
    p: &Palette,
) -> Option<OverlayRender> {
    let popup_height = (view.rows.len().saturating_mul(2) + 7).clamp(12, 26) as u16;
    let popup = popup(b.area, 96, popup_height)?;
    let inner = panel(b, popup, p.accent, p.panel_bg)?;
    put_text(
        b,
        inner.x,
        inner.y,
        inner.width,
        view.title,
        Style::default()
            .fg(p.text)
            .bg(p.panel_bg)
            .add_modifier(Modifier::BOLD),
    );
    let search = Rect::new(inner.x, inner.y + 1, inner.width, 1);
    let filtered = view.filtered;
    put_text(
        b,
        search.x,
        search.y,
        search.width,
        &if view.search_focused {
            " / ".to_owned()
        } else if !view.query.is_empty() {
            format!(" / {}", view.query)
        } else {
            format!(" / {}", view.placeholder)
        },
        Style::default()
            .fg(if view.search_focused {
                p.text
            } else {
                p.overlay0
            })
            .bg(p.panel_bg),
    );
    let count = if filtered.len() == view.rows.len() {
        format!("{} {}", view.rows.len(), view.count_noun)
    } else {
        format!("{}/{} {}", filtered.len(), view.rows.len(), view.count_noun)
    };
    let cursor = if view.search_focused {
        text_editor::render(
            b,
            Rect::new(
                search.x + 3,
                search.y,
                search.width.saturating_sub(4 + display_width(&count)),
                1,
            ),
            view.query,
            Style::default().fg(p.text).bg(p.panel_bg),
        )
    } else {
        None
    };
    put_right_text(
        b,
        search,
        search.y,
        &count,
        Style::default().fg(p.overlay0).bg(p.panel_bg),
    );
    put_text(
        b,
        inner.x,
        inner.y + 2,
        inner.width,
        &"─".repeat(inner.width as usize),
        Style::default().fg(p.surface1).bg(p.panel_bg),
    );
    let body = Rect::new(
        inner.x,
        inner.y + 3,
        inner.width,
        inner.height.saturating_sub(6),
    );
    let visible_count = (body.height / 2).max(1) as usize;
    let selected_position = filtered
        .iter()
        .position(|index| *index == view.selected)
        .unwrap_or(0);
    let start = selected_position
        .saturating_sub(visible_count.saturating_sub(1))
        .min(filtered.len().saturating_sub(visible_count));
    let mut row_hits = Vec::new();
    for (visible, entry_index) in filtered
        .iter()
        .copied()
        .skip(start)
        .take(visible_count)
        .enumerate()
    {
        let entry = &view.rows[entry_index];
        let rect = Rect::new(body.x, body.y + visible as u16 * 2, body.width, 2);
        row_hits.push((rect, entry_index));
        let selected = entry_index == view.selected;
        let style = if selected {
            Style::default().fg(contrast(p)).bg(p.accent)
        } else {
            Style::default().fg(p.text).bg(p.panel_bg)
        };
        b.set_style(rect, style);
        put_text(
            b,
            rect.x,
            rect.y,
            rect.width,
            &format!(" {}", entry.label),
            style.add_modifier(Modifier::BOLD),
        );
        let status = entry.status;
        if !status.is_empty() {
            put_right_text(b, rect, rect.y, status, style);
        }
        put_text(
            b,
            rect.x,
            rect.y + 1,
            rect.width,
            &format!(" {}", entry.detail),
            if selected {
                style
            } else {
                Style::default().fg(p.overlay0).bg(p.panel_bg)
            },
        );
    }
    if filtered.is_empty() {
        put_text(
            b,
            body.x,
            body.y,
            body.width,
            view.empty,
            Style::default().fg(p.overlay0).bg(p.panel_bg),
        );
    }
    if let Some(busy) = view.busy {
        put_text(
            b,
            inner.x,
            inner.bottom() - 3,
            inner.width,
            busy,
            Style::default().fg(p.accent).bg(p.panel_bg),
        );
    } else if let Some(error) = view.error {
        put_text(
            b,
            inner.x,
            inner.bottom() - 3,
            inner.width,
            &format!(" {error}"),
            Style::default().fg(p.red).bg(p.panel_bg),
        );
    }
    let buttons = row(inner, &[10, 12], 2, inner.height.saturating_sub(1));
    let [primary, cancel] = buttons.as_slice() else {
        return None;
    };
    button(
        b,
        *primary,
        view.confirm,
        Style::default()
            .fg(contrast(p))
            .bg(p.accent)
            .add_modifier(Modifier::BOLD),
    );
    button(
        b,
        *cancel,
        " esc cancel ",
        Style::default()
            .fg(p.text)
            .bg(p.surface0)
            .add_modifier(Modifier::BOLD),
    );
    Some(OverlayRender {
        area: popup,
        primary: *primary,
        clear: Rect::default(),
        cancel: *cancel,
        navigator_popup: Rect::default(),
        navigator_search: Rect::default(),
        navigator_rows: Vec::new(),
        worktree_search: search,
        worktree_rows: row_hits,
        cursor: cursor.filter(|_| view.busy.is_none()),
        ..OverlayRender::default()
    })
}

pub(super) fn render_agent_tab_overlay(
    b: &mut Buffer,
    picker: &ClientAgentTabOverlay,
    p: &Palette,
) -> Option<OverlayRender> {
    let filtered = picker.filtered_indices();
    render_picker_overlay(
        b,
        &PickerView {
            title: "new agent tab",
            placeholder: "filter agents",
            count_noun: "agents",
            empty: if picker.entries.is_empty() {
                " no supported agent found on the runtime host's PATH"
            } else {
                " no matching agents"
            },
            confirm: " ↵ open ",
            rows: picker
                .entries
                .iter()
                .map(|entry| PickerRow {
                    label: &entry.kind,
                    status: "",
                    detail: &entry.executable,
                })
                .collect(),
            filtered: &filtered,
            selected: picker.selected,
            query: &picker.query,
            search_focused: true,
            busy: picker.opening.then_some(" opening…"),
            error: picker.error.as_deref(),
        },
        p,
    )
}
