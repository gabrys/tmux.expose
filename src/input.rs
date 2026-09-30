use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::{model::App, ui};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToggleKey {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl ToggleKey {
    pub fn from_tmux_key(value: &str) -> Option<Self> {
        let (mut modifiers, key) = if let Some(key) = value.strip_prefix("M-") {
            (KeyModifiers::ALT, key)
        } else if let Some(key) = value.strip_prefix("C-") {
            (KeyModifiers::CONTROL, key)
        } else {
            (KeyModifiers::NONE, value)
        };

        let code = match key {
            "Esc" => KeyCode::Esc,
            key if key.chars().count() == 1 => {
                let ch = key.chars().next()?;
                if ch.is_ascii_uppercase() {
                    modifiers.insert(KeyModifiers::SHIFT);
                }
                KeyCode::Char(ch)
            }
            _ => return None,
        };

        Some(Self { code, modifiers })
    }

    fn matches(self, key: KeyEvent) -> bool {
        self.code == key.code && self.modifiers == key.modifiers
    }
}

pub fn handle_key(app: &mut App, key: KeyEvent, columns: usize) {
    handle_key_with_toggle(app, key, columns, None);
}

pub fn handle_key_with_toggle(
    app: &mut App,
    key: KeyEvent,
    columns: usize,
    toggle_key: Option<ToggleKey>,
) {
    if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
        app.should_quit = true;
        return;
    }

    if let (KeyCode::Char(digit @ '1'..='9'), KeyModifiers::ALT) = (key.code, key.modifiers) {
        let index = digit as usize - '1' as usize;
        if index < app.visible_session_count() {
            app.selected_index = index;
            app.should_switch = true;
        }
        return;
    }

    if toggle_key.is_some_and(|toggle_key| toggle_key.matches(key)) {
        app.should_quit = true;
        return;
    }

    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => app.should_quit = true,
        (KeyCode::Enter, _) => app.should_switch = true,
        (KeyCode::Left, _) => app.move_left(),
        (KeyCode::Right, _) => app.move_right(),
        (KeyCode::Up, _) => app.move_up(columns),
        (KeyCode::Down, _) => app.move_down(columns),
        _ => {}
    }
}

pub fn handle_mouse(
    app: &mut App,
    mouse: MouseEvent,
    grid_area: Rect,
    min_card_width: Option<u16>,
    forced_columns: Option<usize>,
) {
    if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        return;
    }

    let grid = ui::calculate_grid(
        grid_area,
        app.visible_session_count(),
        min_card_width,
        forced_columns,
    );
    if let Some(index) = grid
        .cards
        .iter()
        .position(|card| contains(*card, mouse.column, mouse.row))
    {
        app.selected_index = index;
        app.should_switch = true;
    }
}

fn contains(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x
        && x < area.x.saturating_add(area.width)
        && y >= area.y
        && y < area.y.saturating_add(area.height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Session;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn session(name: &str) -> Session {
        Session {
            id: format!("%{name}"),
            window_id: format!("@{name}"),
            name: name.to_string(),
            attached: false,
            bell: false,
            preview: Vec::new(),
            preview_error: None,
        }
    }

    fn app(count: usize) -> App {
        App::new(
            (1..=count)
                .map(|index| session(&index.to_string()))
                .collect(),
            None,
        )
    }

    #[test]
    fn alt_digits_select_and_switch_to_matching_card() {
        let mut app = app(9);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('6'), KeyModifiers::ALT),
            3,
        );
        assert_eq!(app.selected_index, 5);
        assert!(app.should_switch);
    }

    #[test]
    fn alt_digit_without_a_matching_card_does_nothing() {
        let mut app = app(3);
        app.selected_index = 1;
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('9'), KeyModifiers::ALT),
            2,
        );
        assert_eq!(app.selected_index, 1);
        assert!(!app.should_switch);
    }

    #[test]
    fn plain_characters_do_nothing() {
        let mut app = app(2);
        handle_key(&mut app, key(KeyCode::Char('f')), 2);
        handle_key(&mut app, key(KeyCode::Char('/')), 2);
        handle_key(&mut app, key(KeyCode::Char('q')), 2);
        assert_eq!(app.selected_index, 0);
        assert!(!app.should_switch);
        assert!(!app.should_quit);
    }

    #[test]
    fn arrow_keys_move_selection() {
        let mut app = app(4);
        handle_key(&mut app, key(KeyCode::Right), 2);
        handle_key(&mut app, key(KeyCode::Down), 2);
        assert_eq!(app.selected_index, 3);
        handle_key(&mut app, key(KeyCode::Left), 2);
        assert_eq!(app.selected_index, 2);
    }

    #[test]
    fn horizontal_navigation_crosses_rows_and_wraps() {
        let mut app = app(4);
        app.selected_index = 3;
        handle_key(&mut app, key(KeyCode::Right), 3);
        assert_eq!(app.selected_index, 0);
        handle_key(&mut app, key(KeyCode::Left), 3);
        assert_eq!(app.selected_index, 3);
    }

    #[test]
    fn enter_marks_app_for_switch() {
        let mut app = app(1);
        handle_key(&mut app, key(KeyCode::Enter), 1);
        assert!(app.should_switch);
    }

    #[test]
    fn esc_and_ctrl_c_quit() {
        let mut escape_app = app(1);
        handle_key(&mut escape_app, key(KeyCode::Esc), 1);
        assert!(escape_app.should_quit);

        let mut ctrl_c_app = app(1);
        handle_key(
            &mut ctrl_c_app,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            1,
        );
        assert!(ctrl_c_app.should_quit);
    }

    #[test]
    fn configured_toggle_key_quits() {
        let mut app = app(1);
        handle_key_with_toggle(
            &mut app,
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::ALT),
            1,
            ToggleKey::from_tmux_key("M-e"),
        );
        assert!(app.should_quit);
    }

    #[test]
    fn left_click_selects_and_switches() {
        let mut app = app(3);
        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 40,
            row: 1,
            modifiers: KeyModifiers::NONE,
        };
        handle_mouse(&mut app, mouse, Rect::new(0, 0, 100, 20), None, Some(3));
        assert_eq!(app.selected_index, 1);
        assert!(app.should_switch);
    }
}
