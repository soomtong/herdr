//! Keyboard focus and global navigation for the agent panel.
//!
//! Two entry points share this module:
//!   * `jump_to_adjacent_agent` — global prev/next bindings that fire from
//!     either Terminal mode or Navigate-mode prefix.
//!   * `enter_agent_panel_focus` / `handle_agent_panel_focus_key` — modal
//!     focus over the existing agent sidebar panel.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::state::{AppState, Mode};
use crate::ui::agent_panel_entries;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentJumpDirection {
    Prev,
    Next,
}

pub(crate) fn jump_to_adjacent_agent(state: &mut AppState, direction: AgentJumpDirection) {
    let entries = agent_panel_entries(state);
    if entries.is_empty() {
        return;
    }

    let focused = state
        .active
        .and_then(|i| state.workspaces.get(i))
        .and_then(|ws| ws.focused_pane_id());

    let current_index =
        focused.and_then(|pane_id| entries.iter().position(|entry| entry.pane_id == pane_id));

    let target_index = match (direction, current_index) {
        (AgentJumpDirection::Next, Some(i)) => (i + 1) % entries.len(),
        (AgentJumpDirection::Next, None) => 0,
        (AgentJumpDirection::Prev, Some(0)) => entries.len() - 1,
        (AgentJumpDirection::Prev, Some(i)) => i - 1,
        (AgentJumpDirection::Prev, None) => entries.len() - 1,
    };

    let target = &entries[target_index];
    let ws_idx = target.ws_idx;
    let tab_idx = target.tab_idx;
    let pane_id = target.pane_id;

    state.switch_workspace(ws_idx);
    state.switch_tab(tab_idx);
    state.focus_pane(pane_id);
}

pub(crate) fn enter_agent_panel_focus(state: &mut AppState) {
    let entries = agent_panel_entries(state);
    if entries.is_empty() {
        return;
    }

    if state.sidebar_collapsed {
        state.sidebar_collapsed = false;
    }

    let focused_pane = state
        .active
        .and_then(|i| state.workspaces.get(i))
        .and_then(|ws| ws.focused_pane_id());

    let initial = focused_pane
        .and_then(|pane_id| entries.iter().position(|e| e.pane_id == pane_id))
        .unwrap_or(0);

    state.agent_panel_selected = Some(initial);
    state.mode = Mode::AgentPanelFocus;
}

pub(crate) fn handle_agent_panel_focus_key(state: &mut AppState, key: KeyEvent) {
    let entries = agent_panel_entries(state);
    if entries.is_empty() {
        // Entries vanished while focused — return to terminal mode.
        state.agent_panel_selected = None;
        state.mode = Mode::Terminal;
        return;
    }

    // Clamp selection into bounds in case entries shrank since entry.
    let mut selected = state
        .agent_panel_selected
        .unwrap_or(0)
        .min(entries.len() - 1);

    match key.code {
        KeyCode::Up => {
            selected = selected.saturating_sub(1);
            state.agent_panel_selected = Some(selected);
        }
        KeyCode::Down => {
            if selected + 1 < entries.len() {
                selected += 1;
            }
            state.agent_panel_selected = Some(selected);
        }
        KeyCode::Char('p') if key.modifiers == KeyModifiers::CONTROL => {
            selected = selected.saturating_sub(1);
            state.agent_panel_selected = Some(selected);
        }
        KeyCode::Char('n') if key.modifiers == KeyModifiers::CONTROL => {
            if selected + 1 < entries.len() {
                selected += 1;
            }
            state.agent_panel_selected = Some(selected);
        }
        KeyCode::Enter => {
            let target = &entries[selected];
            let ws_idx = target.ws_idx;
            let tab_idx = target.tab_idx;
            let pane_id = target.pane_id;
            state.switch_workspace(ws_idx);
            state.switch_tab(tab_idx);
            state.focus_pane(pane_id);
            state.agent_panel_selected = None;
            state.mode = Mode::Terminal;
        }
        KeyCode::Esc => {
            state.agent_panel_selected = None;
            state.mode = Mode::Terminal;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::Agent;
    use crate::layout::PaneId;
    use crate::workspace::Workspace;

    fn state_with_agent_panes(count: usize) -> AppState {
        let mut state = AppState::test_new();
        state.workspaces = (0..count)
            .map(|i| {
                let mut ws = Workspace::test_new(&format!("ws-{i}"));
                let pane = ws.tabs[0].root_pane;
                ws.tabs[0]
                    .panes
                    .get_mut(&pane)
                    .expect("root pane must exist")
                    .detected_agent = Some(Agent::Pi);
                ws
            })
            .collect();
        if count > 0 {
            state.active = Some(0);
            state.selected = 0;
        }
        state.mode = Mode::Terminal;
        state
    }

    #[test]
    fn next_agent_jumps_to_first_when_focused_pane_not_in_entries() {
        let mut state = state_with_agent_panes(3);
        // Clear `detected_agent` on workspace 0's pane so its pane_id is not
        // in `agent_panel_entries`, simulating a focused pane that's not an
        // agent. Keep workspace 0 active.
        let pane0 = state.workspaces[0].tabs[0].root_pane;
        state.workspaces[0].tabs[0]
            .panes
            .get_mut(&pane0)
            .unwrap()
            .detected_agent = None;
        // Active stays on workspace 0; its focused pane is the (non-agent) root.
        state.active = Some(0);

        let entries_before = agent_panel_entries(&state);
        // workspace 0 has no agent pane, so entries are workspaces 1 and 2.
        assert_eq!(entries_before.len(), 2);

        jump_to_adjacent_agent(&mut state, AgentJumpDirection::Next);

        // Should have jumped to the first entry (workspace 1).
        assert_eq!(state.active, Some(1));
        let ws = &state.workspaces[1];
        assert_eq!(ws.focused_pane_id(), Some(ws.tabs[0].root_pane));
    }

    #[test]
    fn next_agent_wraps_around_at_end() {
        let mut state = state_with_agent_panes(3);
        // Focus the last entry: workspace 2.
        state.active = Some(2);
        let last_pane = state.workspaces[2].tabs[0].root_pane;
        state.workspaces[2].layout.focus_pane(last_pane);

        jump_to_adjacent_agent(&mut state, AgentJumpDirection::Next);

        // Wraps to first entry (workspace 0).
        assert_eq!(state.active, Some(0));
        let first_pane = state.workspaces[0].tabs[0].root_pane;
        assert_eq!(state.workspaces[0].focused_pane_id(), Some(first_pane));
    }

    #[test]
    fn previous_agent_wraps_around_at_start() {
        let mut state = state_with_agent_panes(3);
        // Focus first entry: workspace 0.
        state.active = Some(0);
        let first_pane = state.workspaces[0].tabs[0].root_pane;
        state.workspaces[0].layout.focus_pane(first_pane);

        jump_to_adjacent_agent(&mut state, AgentJumpDirection::Prev);

        // Wraps to last entry (workspace 2).
        assert_eq!(state.active, Some(2));
        let last_pane = state.workspaces[2].tabs[0].root_pane;
        assert_eq!(state.workspaces[2].focused_pane_id(), Some(last_pane));
    }

    #[test]
    fn jump_is_noop_when_no_agents() {
        let mut state = state_with_agent_panes(0);
        let active_before = state.active;
        let mode_before = state.mode;

        jump_to_adjacent_agent(&mut state, AgentJumpDirection::Next);
        jump_to_adjacent_agent(&mut state, AgentJumpDirection::Prev);

        assert_eq!(state.active, active_before);
        assert_eq!(state.mode, mode_before);
    }

    fn setup_state_with_two_agents_focused_on_second() -> (AppState, PaneId) {
        let mut state = state_with_agent_panes(2);
        // Activate workspace 1 and ensure its focused pane is its (agent) root.
        state.active = Some(1);
        state.selected = 1;
        let second_pane = state.workspaces[1].tabs[0].root_pane;
        state.workspaces[1].layout.focus_pane(second_pane);
        (state, second_pane)
    }

    fn setup_state_with_two_agents_and_one_shell_focused() -> (AppState, PaneId) {
        // Two agent workspaces plus a third workspace whose focused pane is
        // a plain shell (no detected_agent), so the focused pane is NOT in
        // `agent_panel_entries`.
        let mut state = state_with_agent_panes(2);
        let mut shell_ws = Workspace::test_new("ws-shell");
        let shell_pane = shell_ws.tabs[0].root_pane;
        // Leave detected_agent as None so this pane is not in entries.
        shell_ws.layout.focus_pane(shell_pane);
        state.workspaces.push(shell_ws);
        state.active = Some(2);
        state.selected = 2;
        (state, shell_pane)
    }

    #[test]
    fn enter_focus_initializes_selection_from_focused_pane() {
        let (mut state, second_agent_pane) = setup_state_with_two_agents_focused_on_second();
        enter_agent_panel_focus(&mut state);
        assert_eq!(state.mode, Mode::AgentPanelFocus);
        // entries are ordered consistently — the second agent pane is at index 1.
        assert_eq!(state.agent_panel_selected, Some(1));
        let _ = second_agent_pane;
    }

    #[test]
    fn enter_focus_falls_back_to_zero_when_focused_pane_not_in_entries() {
        let (mut state, _) = setup_state_with_two_agents_and_one_shell_focused();
        enter_agent_panel_focus(&mut state);
        assert_eq!(state.mode, Mode::AgentPanelFocus);
        assert_eq!(state.agent_panel_selected, Some(0));
    }

    #[test]
    fn enter_focus_noop_when_no_agents() {
        let mut state = AppState::test_new();
        state.mode = Mode::Terminal;
        enter_agent_panel_focus(&mut state);
        assert_eq!(state.mode, Mode::Terminal);
        assert_eq!(state.agent_panel_selected, None);
    }

    #[test]
    fn enter_focus_expands_collapsed_sidebar() {
        let (mut state, _) = setup_state_with_two_agents_and_one_shell_focused();
        state.sidebar_collapsed = true;
        enter_agent_panel_focus(&mut state);
        assert!(!state.sidebar_collapsed);
        assert_eq!(state.mode, Mode::AgentPanelFocus);
    }

    fn setup_state_with_two_agents_focused_on_first() -> (AppState, PaneId) {
        let mut state = state_with_agent_panes(2);
        state.active = Some(0);
        state.selected = 0;
        let first_pane = state.workspaces[0].tabs[0].root_pane;
        state.workspaces[0].layout.focus_pane(first_pane);
        (state, first_pane)
    }

    fn setup_state_with_two_agents_focused_on_last() -> (AppState, PaneId) {
        setup_state_with_two_agents_focused_on_second()
    }

    fn currently_focused_pane(state: &AppState) -> Option<PaneId> {
        state
            .active
            .and_then(|i| state.workspaces.get(i))
            .and_then(|ws| ws.focused_pane_id())
    }

    #[test]
    fn navigation_clamps_at_top_bound() {
        let (mut state, _) = setup_state_with_two_agents_focused_on_first();
        enter_agent_panel_focus(&mut state);
        assert_eq!(state.agent_panel_selected, Some(0));

        handle_agent_panel_focus_key(
            &mut state,
            KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
        );
        assert_eq!(state.agent_panel_selected, Some(0));
    }

    #[test]
    fn navigation_clamps_at_bottom_bound() {
        let (mut state, _) = setup_state_with_two_agents_focused_on_last();
        enter_agent_panel_focus(&mut state);
        assert_eq!(state.agent_panel_selected, Some(1));

        handle_agent_panel_focus_key(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        assert_eq!(state.agent_panel_selected, Some(1));
    }

    #[test]
    fn ctrl_n_moves_down() {
        let (mut state, _) = setup_state_with_two_agents_focused_on_first();
        enter_agent_panel_focus(&mut state);

        handle_agent_panel_focus_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
        );
        assert_eq!(state.agent_panel_selected, Some(1));
    }

    #[test]
    fn ctrl_p_moves_up() {
        let (mut state, _) = setup_state_with_two_agents_focused_on_last();
        enter_agent_panel_focus(&mut state);

        handle_agent_panel_focus_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
        );
        assert_eq!(state.agent_panel_selected, Some(0));
    }

    #[test]
    fn enter_jumps_and_returns_to_terminal_mode() {
        let (mut state, _) = setup_state_with_two_agents_focused_on_first();
        enter_agent_panel_focus(&mut state);
        handle_agent_panel_focus_key(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        assert_eq!(state.agent_panel_selected, Some(1));

        handle_agent_panel_focus_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );
        assert_eq!(state.mode, Mode::Terminal);
        assert_eq!(state.agent_panel_selected, None);
        let entries = agent_panel_entries(&state);
        assert_eq!(currently_focused_pane(&state), Some(entries[1].pane_id));
    }

    #[test]
    fn esc_returns_to_terminal_mode_without_jumping() {
        let (mut state, originally_focused) = setup_state_with_two_agents_focused_on_first();
        enter_agent_panel_focus(&mut state);
        handle_agent_panel_focus_key(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );

        handle_agent_panel_focus_key(
            &mut state,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
        );
        assert_eq!(state.mode, Mode::Terminal);
        assert_eq!(state.agent_panel_selected, None);
        assert_eq!(currently_focused_pane(&state), Some(originally_focused));
    }

    #[test]
    fn empty_entries_during_focus_exit_mode() {
        let (mut state, _) = setup_state_with_two_agents_focused_on_first();
        enter_agent_panel_focus(&mut state);

        // Simulate all agents going away (e.g. external close).
        for ws in &mut state.workspaces {
            ws.tabs.clear();
        }

        handle_agent_panel_focus_key(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        assert_eq!(state.mode, Mode::Terminal);
        assert_eq!(state.agent_panel_selected, None);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let (mut state, _) = setup_state_with_two_agents_focused_on_first();
        enter_agent_panel_focus(&mut state);
        let before = state.agent_panel_selected;

        handle_agent_panel_focus_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('z'), KeyModifiers::empty()),
        );
        assert_eq!(state.mode, Mode::AgentPanelFocus);
        assert_eq!(state.agent_panel_selected, before);
    }

    #[test]
    fn scope_change_clamps_selection() {
        let (mut state, _) = setup_state_with_two_agents_focused_on_first();
        enter_agent_panel_focus(&mut state);
        state.agent_panel_selected = Some(1);

        // Simulate one agent going away (e.g. external close shrinking entries to length 1).
        if let Some(ws) = state.workspaces.last_mut() {
            ws.tabs.clear();
        }

        handle_agent_panel_focus_key(
            &mut state,
            KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
        );
        // After clamp inside the handler, selected should be 0 (the only remaining entry).
        assert_eq!(state.agent_panel_selected, Some(0));
    }
}
