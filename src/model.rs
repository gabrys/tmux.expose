#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// Stable tmux pane id (for example `%12`).
    pub id: String,
    /// Stable tmux window id containing this pane (for example `@3`).
    pub window_id: String,
    pub name: String,
    pub attached: bool,
    pub bell: bool,
    pub preview: Vec<String>,
    pub preview_error: Option<String>,
}

#[derive(Debug)]
pub struct App {
    pub sessions: Vec<Session>,
    pub selected_index: usize,
    pub current_pane_id: Option<String>,
    pub should_quit: bool,
    pub should_switch: bool,
    pub error: Option<String>,
    /// When true, the picker uses modal vim navigation (hjkl to move, `/` to search).
    pub vim_keys: bool,
    search_query: Option<String>,
    /// True while actively typing a query. In vim mode this is distinct from
    /// `search_query` being set: Esc leaves editing (so hjkl moves again) but
    /// keeps `search_query` as an applied filter, matching how Telescope-style
    /// pickers commit a search into normal-mode browsing instead of discarding
    /// it. Default mode has no normal-mode navigation to return to, so the two
    /// always stay in lockstep there — see `is_searching`.
    editing_search: bool,
}

impl App {
    pub fn new(sessions: Vec<Session>, current_pane_id: Option<String>) -> Self {
        let selected_index = current_pane_id
            .as_ref()
            .and_then(|id| sessions.iter().position(|pane| &pane.id == id))
            .unwrap_or(0);

        Self {
            sessions,
            selected_index,
            current_pane_id,
            should_quit: false,
            should_switch: false,
            error: None,
            vim_keys: false,
            search_query: None,
            editing_search: false,
        }
    }

    pub fn selected_session(&self) -> Option<&Session> {
        self.visible_sessions().get(self.selected_index).copied()
    }

    pub fn visible_sessions(&self) -> Vec<&Session> {
        match self.search_query.as_deref() {
            Some(query) => self
                .sessions
                .iter()
                .filter(|session| fuzzy_matches(&session.name, query))
                .collect(),
            None => self.sessions.iter().collect(),
        }
    }

    pub fn visible_session_count(&self) -> usize {
        self.visible_sessions().len()
    }

    pub fn start_search(&mut self) {
        self.search_query = Some(String::new());
        self.editing_search = true;
        self.selected_index = 0;
    }

    pub fn push_search_char(&mut self, ch: char) {
        if let Some(query) = &mut self.search_query {
            query.push(ch);
            self.selected_index = 0;
        }
    }

    pub fn pop_search_char(&mut self) {
        if let Some(query) = &mut self.search_query {
            query.pop();
            self.selected_index = 0;
        }
    }

    pub fn clear_search(&mut self) {
        self.search_query = None;
        self.editing_search = false;
        self.selected_index = 0;
    }

    /// Leaves text-entry but keeps `search_query` as an applied filter — vim
    /// mode's Esc-while-searching, so hjkl navigates the filtered results
    /// instead of discarding them.
    pub fn stop_editing_search(&mut self) {
        self.editing_search = false;
    }

    /// Whether keystrokes should be treated as search text-entry right now.
    /// Also drives the toggle-key/typeable-filter check in `input.rs`. In
    /// vim mode this is `false` while a filter is applied but not being
    /// edited; in default mode it always matches `search_query.is_some()`,
    /// since default mode has no normal-mode navigation to drop into.
    pub fn is_searching(&self) -> bool {
        self.editing_search
    }

    pub fn search_text(&self) -> Option<&str> {
        self.search_query.as_deref()
    }

    pub fn replace_sessions(&mut self, sessions: Vec<Session>) {
        let selected_id = self.selected_session().map(|session| session.id.clone());
        self.sessions = sessions;

        if self.visible_session_count() == 0 {
            self.selected_index = 0;
            return;
        }

        self.selected_index = selected_id
            .and_then(|id| {
                self.visible_sessions()
                    .into_iter()
                    .position(|pane| pane.id == id)
            })
            .unwrap_or_else(|| self.selected_index.min(self.visible_session_count() - 1));
    }

    pub fn replace_sessions_preserving_preview_for(
        &mut self,
        mut sessions: Vec<Session>,
        preserved_session_id: Option<&str>,
    ) {
        if let Some(preserved_session_id) = preserved_session_id
            && let Some(previous) = self
                .sessions
                .iter()
                .find(|session| session.id == preserved_session_id)
            && let Some(next) = sessions
                .iter_mut()
                .find(|session| session.id == preserved_session_id)
        {
            next.preview = previous.preview.clone();
            next.preview_error = previous.preview_error.clone();
        }

        self.replace_sessions(sessions);
    }

    pub fn move_left(&mut self) {
        let visible_count = self.visible_session_count();
        if visible_count == 0 {
            self.selected_index = 0;
        } else {
            self.selected_index = (self.selected_index + visible_count - 1) % visible_count;
        }
    }

    pub fn move_right(&mut self) {
        let visible_count = self.visible_session_count();
        if visible_count == 0 {
            self.selected_index = 0;
        } else {
            self.selected_index = (self.selected_index + 1) % visible_count;
        }
    }

    pub fn move_up(&mut self, columns: usize) {
        let columns = columns.max(1);
        if self.selected_index >= columns {
            self.selected_index -= columns;
        }
    }

    pub fn move_down(&mut self, columns: usize) {
        let columns = columns.max(1);
        let visible_count = self.visible_session_count();
        if visible_count == 0 {
            return;
        }

        let last_index = visible_count - 1;
        let current_row = self.selected_index / columns;
        let last_row = last_index / columns;
        if current_row < last_row {
            self.selected_index = self.selected_index.saturating_add(columns).min(last_index);
        }
    }
}

fn fuzzy_matches(name: &str, query: &str) -> bool {
    let query = query.to_lowercase();
    if query.is_empty() {
        return true;
    }

    let name = name.to_lowercase();
    let mut name_chars = name.chars();
    query
        .chars()
        .all(|query_ch| name_chars.any(|name_ch| name_ch == query_ch))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn selects_current_pane_when_present() {
        let app = App::new(
            vec![session("dev"), session("logs"), session("notes")],
            Some("%logs".to_string()),
        );

        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn horizontal_navigation_wraps_at_list_edges() {
        let mut app = App::new(vec![session("one"), session("two"), session("three")], None);

        app.move_left();
        assert_eq!(app.selected_index, 2);

        app.move_right();
        assert_eq!(app.selected_index, 0);

        app.move_right();
        app.move_right();
        assert_eq!(app.selected_index, 2);
    }

    #[test]
    fn vertical_navigation_still_clamps_at_grid_edges() {
        let mut app = App::new(vec![session("one"), session("two"), session("three")], None);
        app.selected_index = 2;

        app.move_down(2);
        assert_eq!(app.selected_index, 2);

        app.move_up(2);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn preserves_selected_pane_by_id_after_refresh() {
        let mut app = App::new(
            vec![session("dev"), session("logs"), session("notes")],
            None,
        );
        app.selected_index = 1;

        app.replace_sessions(vec![session("new"), session("logs"), session("dev")]);

        assert_eq!(app.selected_session().unwrap().name, "logs");
    }

    #[test]
    fn preserves_preview_for_matching_session_after_refresh() {
        let mut app = App::new(
            vec![session("dev"), session("logs")],
            Some("%dev".to_string()),
        );
        app.sessions[0].preview = vec!["snapshot".to_string()];
        app.sessions[0].preview_error = None;

        let mut refreshed_dev = session("dev");
        refreshed_dev.preview = Vec::new();
        refreshed_dev.preview_error = Some("Current session preview disabled".to_string());

        let mut refreshed_logs = session("logs");
        refreshed_logs.preview = vec!["live".to_string()];

        app.replace_sessions_preserving_preview_for(
            vec![refreshed_dev, refreshed_logs],
            Some("%dev"),
        );

        assert_eq!(app.sessions[0].preview, vec!["snapshot".to_string()]);
        assert_eq!(app.sessions[0].preview_error, None);
        assert_eq!(app.sessions[1].preview, vec!["live".to_string()]);
    }

    #[test]
    fn search_filters_sessions_by_fuzzy_name() {
        let mut app = App::new(
            vec![
                session("backend-api"),
                session("frontend"),
                session("database"),
            ],
            None,
        );

        app.start_search();
        app.push_search_char('b');
        app.push_search_char('a');

        let names: Vec<&str> = app
            .visible_sessions()
            .into_iter()
            .map(|session| session.name.as_str())
            .collect();
        assert_eq!(names, vec!["backend-api", "database"]);
    }

    #[test]
    fn selected_session_uses_filtered_selection() {
        let mut app = App::new(
            vec![session("backend"), session("frontend"), session("database")],
            None,
        );

        app.start_search();
        app.push_search_char('f');

        assert_eq!(app.selected_index, 0);
        assert_eq!(app.selected_session().unwrap().name, "frontend");
    }

    #[test]
    fn clearing_search_restores_all_sessions() {
        let mut app = App::new(vec![session("backend"), session("frontend")], None);

        app.start_search();
        app.push_search_char('f');
        app.clear_search();

        assert!(!app.is_searching());
        assert_eq!(app.visible_session_count(), 2);
    }

    #[test]
    fn up_from_first_row_keeps_selection_in_place() {
        let mut app = App::new(vec![session("one"), session("two"), session("three")], None);
        app.selected_index = 1;

        app.move_up(2);

        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn down_to_incomplete_row_selects_nearest_card() {
        let mut app = App::new(
            vec![
                session("one"),
                session("two"),
                session("three"),
                session("four"),
                session("five"),
            ],
            None,
        );
        app.selected_index = 2;

        app.move_down(3);

        assert_eq!(app.selected_index, 4);
    }
}
