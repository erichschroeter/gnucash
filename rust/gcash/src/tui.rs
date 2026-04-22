use crate::config::{Action, AppSettings};
use crate::error::AppError;
use crossterm::{
    event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use gnucash_engine::domain::{Account, AccountId, Ledger, Transaction};
use num_traits::ToPrimitive;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Tabs},
    Terminal,
};
use std::{
    collections::HashMap,
    io,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

/// TUI events handled by the async event loop.
#[derive(Clone, Copy, Debug)]
pub enum Event {
    Tick,
    Key(event::KeyEvent),
}

/// Application view states for Model-View-Update architecture.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum AppState {
    View,
    Edit,
    Search,
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditTarget {
    Account(AccountId),
    Transaction(gnucash_engine::domain::TransactionId),
}

pub struct EditState {
    pub target: EditTarget,
    pub fields: Vec<String>,
    pub active_field_index: usize,
    pub cursor_position: usize,
}

/// The core Model for the interactive application.
pub struct App {
    pub state: AppState,
    pub edit_state: Option<EditState>,
    pub should_quit: bool,
    pub ledger: Ledger,
    pub tab_index: usize,
    pub active_accounts: Vec<AccountId>,
    pub key_map: HashMap<String, Action>,
    pub table_state: ratatui::widgets::TableState,
    pub search_query: String,
    pub pending_keys: String,
    pub tab_scroll_offset: usize,
}

impl App {
    pub fn new(ledger: Ledger, key_map: HashMap<String, Action>) -> Self {
        let mut active_accounts: Vec<AccountId> = ledger.accounts.iter().map(|a| a.id).collect();
        // Sort active accounts by name for consistent tab order
        active_accounts.sort_by_cached_key(|id| {
            ledger
                .accounts
                .iter()
                .find(|a| a.id == *id)
                .map(|a| a.name.clone())
                .unwrap_or_default()
        });

        let mut table_state = ratatui::widgets::TableState::default();
        table_state.select(Some(0));

        Self {
            state: AppState::View,
            edit_state: None,
            should_quit: false,
            ledger,
            tab_index: 0,
            active_accounts,
            key_map,
            table_state,
            search_query: String::new(),
            pending_keys: String::new(),
            tab_scroll_offset: 0,
        }
    }

    fn account_matches_query(&self, account: &Account, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        account.name.to_lowercase().contains(query)
            || format!("{:?}", account.account_type)
                .to_lowercase()
                .contains(query)
            || format!("{:?}", account.id).to_lowercase().contains(query)
    }

    fn transaction_matches_query(
        &self,
        tx: &Transaction,
        active_id: &AccountId,
        query: &str,
    ) -> bool {
        if query.is_empty() {
            return true;
        }
        // Date
        if tx
            .date()
            .format("%Y-%m-%d")
            .to_string()
            .to_lowercase()
            .contains(query)
        {
            return true;
        }
        // Description
        if tx.description().to_lowercase().contains(query) {
            return true;
        }
        // Transfer
        let other_splits: Vec<_> = tx
            .splits()
            .iter()
            .filter(|s| s.account_id != *active_id)
            .collect();
        let transfer = if other_splits.len() == 1 {
            self.get_account_name(&other_splits[0].account_id)
        } else if other_splits.len() > 1 {
            "-- Split --".to_string()
        } else {
            "None".to_string()
        };
        if transfer.to_lowercase().contains(query) {
            return true;
        }
        // Amount
        let active_split = tx
            .splits()
            .iter()
            .find(|s| s.account_id == *active_id)
            .unwrap();
        let amount_val = format!("{:.2}", active_split.amount.to_f64().unwrap_or(0.0));
        if amount_val.contains(query) {
            return true;
        }

        false
    }

    fn current_row_count(&self) -> usize {
        let query = self.search_query.to_lowercase();
        if self.tab_index == 0 {
            self.ledger
                .accounts
                .iter()
                .filter(|a| self.account_matches_query(a, &query))
                .count()
        } else {
            let active_id = self.active_accounts[self.tab_index - 1];
            self.ledger
                .transactions
                .iter()
                .filter(|tx| tx.splits().iter().any(|s| s.account_id == active_id))
                .filter(|tx| self.transaction_matches_query(tx, &active_id, &query))
                .count()
        }
    }

    /// Update logic applied when a new event is received.
    pub fn update(&mut self, event: Event) {
        if let Event::Key(key) = event {
            // Only trigger on key press down
            if key.kind == KeyEventKind::Press {
                let key_str = key_to_string(&key);

                // If in Search mode, handle keys directly and skip sequence logic
                if self.state == AppState::Search {
                    match key.code {
                        KeyCode::Char(c) => self.search_query.push(c),
                        KeyCode::Backspace => {
                            self.search_query.pop();
                        }
                        KeyCode::Enter => {
                            self.state = AppState::View;
                            self.open_selected_entry();
                        }
                        KeyCode::Esc => {
                            self.search_query.clear();
                            self.state = AppState::View;
                            self.table_state.select(Some(0));
                        }
                        _ => {}
                    }
                    // Reset selection when filtering changes
                    let max = self.current_row_count();
                    if let Some(i) = self.table_state.selected() {
                        if max == 0 {
                            self.table_state.select(None);
                        } else if i >= max {
                            self.table_state.select(Some(max - 1));
                        }
                    } else if max > 0 {
                        self.table_state.select(Some(0));
                    }
                    return;
                }

                // Handle sequence logic for View and Help modes
                self.pending_keys.push_str(&key_str);
                let mut action = self.key_map.get(&self.pending_keys).copied();

                if action.is_none() {
                    let is_prefix = self.key_map.keys().any(|k| k.starts_with(&self.pending_keys));
                    if !is_prefix {
                        self.pending_keys = key_str.clone();
                        action = self.key_map.get(&self.pending_keys).copied();
                    }
                }

                if action.is_some() {
                    self.pending_keys.clear();
                }

                if self.state == AppState::Help {
                    if key.code == KeyCode::Esc
                        || key.code == KeyCode::Enter
                        || key.code == KeyCode::Char('q')
                        || key.code == KeyCode::Char('?')
                    {
                        self.state = AppState::View;
                        self.pending_keys.clear();
                        return;
                    }

                    if let Some(act) = action {
                        match act {
                            Action::Quit | Action::ViewMode | Action::ShowHelp => {
                                self.state = AppState::View;
                                return;
                            }
                            _ => {}
                        }
                    }
                    return;
                }

                if self.state == AppState::Edit {
                    if let Some(ref mut edit_state) = self.edit_state {
                        if action == Some(Action::FocusNext) || key.code == KeyCode::Tab {
                            let len = edit_state.fields.len();
                            edit_state.active_field_index =
                                (edit_state.active_field_index + 1) % len;
                            edit_state.cursor_position = edit_state.fields
                                [edit_state.active_field_index]
                                .chars()
                                .count();
                        } else if action == Some(Action::FocusPrev) || key.code == KeyCode::BackTab
                        {
                            let len = edit_state.fields.len();
                            edit_state.active_field_index =
                                (edit_state.active_field_index + len - 1) % len;
                            edit_state.cursor_position = edit_state.fields
                                [edit_state.active_field_index]
                                .chars()
                                .count();
                        } else if action == Some(Action::MoveLeft) || key.code == KeyCode::Left {
                            if edit_state.cursor_position > 0 {
                                edit_state.cursor_position -= 1;
                            }
                        } else if action == Some(Action::MoveRight) || key.code == KeyCode::Right {
                            let len = edit_state.fields[edit_state.active_field_index]
                                .chars()
                                .count();
                            if edit_state.cursor_position < len {
                                edit_state.cursor_position += 1;
                            }
                        } else {
                            match key.code {
                                KeyCode::Char(c) => {
                                    let field =
                                        &mut edit_state.fields[edit_state.active_field_index];
                                    let mut chars: Vec<char> = field.chars().collect();
                                    chars.insert(edit_state.cursor_position, c);
                                    *field = chars.into_iter().collect();
                                    edit_state.cursor_position += 1;
                                }
                                KeyCode::Backspace => {
                                    if edit_state.cursor_position > 0 {
                                        let field =
                                            &mut edit_state.fields[edit_state.active_field_index];
                                        let mut chars: Vec<char> = field.chars().collect();
                                        chars.remove(edit_state.cursor_position - 1);
                                        *field = chars.into_iter().collect();
                                        edit_state.cursor_position -= 1;
                                    }
                                }
                                KeyCode::Delete => {
                                    let field =
                                        &mut edit_state.fields[edit_state.active_field_index];
                                    let mut chars: Vec<char> = field.chars().collect();
                                    if edit_state.cursor_position < chars.len() {
                                        chars.remove(edit_state.cursor_position);
                                        *field = chars.into_iter().collect();
                                    }
                                }
                                KeyCode::Enter => {
                                    // Save changes
                                    let mut new_tx_draft = None;
                                    let mut target_tx_id = None;
                                    match edit_state.target.clone() {
                                        EditTarget::Account(acc_id) => {
                                            if let Some(acc) = self
                                                .ledger
                                                .accounts
                                                .iter_mut()
                                                .find(|a| a.id == acc_id)
                                            {
                                                acc.name = edit_state.fields[0].clone();
                                            }
                                        }
                                        EditTarget::Transaction(tx_id) => {
                                            if let Some(tx) = self
                                                .ledger
                                                .transactions
                                                .iter()
                                                .find(|t| t.id() == tx_id)
                                            {
                                                let mut draft = tx.clone().into_draft();
                                                // Parse date (simple format check)
                                                if let Ok(date) = chrono::NaiveDate::parse_from_str(
                                                    &edit_state.fields[0],
                                                    "%Y-%m-%d",
                                                ) {
                                                    let datetime = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(date.and_hms_opt(0, 0, 0).unwrap(), chrono::Utc);
                                                    draft = draft.with_date(datetime);
                                                }
                                                draft = draft
                                                    .with_description(edit_state.fields[1].clone());

                                                // Parse Amount
                                                if let Ok(val) = edit_state.fields[2].parse::<f64>()
                                                {
                                                    use num_rational::Rational64;
                                                    let amount = Rational64::new(
                                                        (val * 100.0).round() as i64,
                                                        100,
                                                    );

                                                    let active_id =
                                                        self.active_accounts[self.tab_index - 1];
                                                    // Find and update active split
                                                    if let Some(pos) = draft
                                                        .splits
                                                        .iter()
                                                        .position(|s| s.account_id == active_id)
                                                    {
                                                        draft.splits[pos].amount = amount;

                                                        // If exactly 2 splits, balance it
                                                        if draft.splits.len() == 2 {
                                                            let other_pos = 1 - pos;
                                                            draft.splits[other_pos].amount =
                                                                -amount;
                                                        }
                                                    }
                                                }

                                                new_tx_draft = Some(draft);
                                                target_tx_id = Some(tx_id);
                                            }
                                        }
                                    }

                                    if let Some(draft) = new_tx_draft {
                                        if let Ok(valid_tx) = draft.validate() {
                                            if let Some(id) = target_tx_id {
                                                if let Some(pos) = self
                                                    .ledger
                                                    .transactions
                                                    .iter()
                                                    .position(|t| t.id() == id)
                                                {
                                                    self.ledger.transactions[pos] = valid_tx;
                                                }
                                            }
                                        }
                                    }

                                    self.state = AppState::View;
                                    self.edit_state = None;
                                }
                                KeyCode::Esc => {
                                    self.state = AppState::View;
                                    self.edit_state = None;
                                }
                                _ => {}
                            }
                        }
                    } else if key.code == KeyCode::Esc {
                        self.state = AppState::View;
                    }
                    return;
                }

                if let Some(action) = action {
                    match action {
                        Action::Quit => self.should_quit = true,
                        Action::ViewMode => self.state = AppState::View,
                        Action::Search => self.state = AppState::Search,
                        Action::ShowHelp => self.state = AppState::Help,
                        Action::EditMode | Action::EditEntry => {
                            if self.state == AppState::View {
                                if self.tab_index == 0 {
                                    if let Some(acc_id) = self.get_selected_account() {
                                        if let Some(acc) =
                                            self.ledger.accounts.iter().find(|a| a.id == acc_id)
                                        {
                                            self.edit_state = Some(EditState {
                                                target: EditTarget::Account(acc_id),
                                                fields: vec![acc.name.clone()],
                                                active_field_index: 0,
                                                cursor_position: acc.name.chars().count(),
                                            });
                                            self.state = AppState::Edit;
                                        }
                                    }
                                } else {
                                    if let Some(tx_id) = self.get_selected_transaction() {
                                        if let Some(tx) = self
                                            .ledger
                                            .transactions
                                            .iter()
                                            .find(|t| t.id() == tx_id)
                                        {
                                            let active_id =
                                                self.active_accounts[self.tab_index - 1];
                                            let active_split = tx
                                                .splits()
                                                .iter()
                                                .find(|s| s.account_id == active_id)
                                                .expect("Active split not found");
                                            let amount_str = format!(
                                                "{:.2}",
                                                active_split.amount.to_f64().unwrap_or(0.0)
                                            );
                                            let fields = vec![
                                                tx.date().format("%Y-%m-%d").to_string(),
                                                tx.description().to_string(),
                                                amount_str,
                                            ];
                                            let cursor_position = fields[0].chars().count();
                                            self.edit_state = Some(EditState {
                                                target: EditTarget::Transaction(tx_id),
                                                fields,
                                                active_field_index: 0,
                                                cursor_position,
                                            });
                                            self.state = AppState::Edit;
                                        }
                                    }
                                }
                            }
                        }
                        Action::OpenEntry => {
                            self.open_selected_entry();
                        }
                        Action::MoveRight | Action::FocusNext => {
                            let total_tabs = self.active_accounts.len() + 1;
                            self.tab_index = (self.tab_index + 1) % total_tabs;
                            self.table_state.select(Some(0));
                        }
                        Action::MoveLeft | Action::FocusPrev => {
                            let total_tabs = self.active_accounts.len() + 1;
                            if self.tab_index == 0 {
                                self.tab_index = total_tabs - 1;
                            } else {
                                self.tab_index -= 1;
                            }
                            self.table_state.select(Some(0));
                        }
                        Action::MoveDown => {
                            let max = self.current_row_count();
                            if max > 0 {
                                let i = match self.table_state.selected() {
                                    Some(i) => {
                                        if i >= max - 1 {
                                            max - 1
                                        } else {
                                            i + 1
                                        }
                                    }
                                    None => 0,
                                };
                                self.table_state.select(Some(i));
                            }
                        }
                        Action::MoveUp => {
                            let max = self.current_row_count();
                            if max > 0 {
                                let i = match self.table_state.selected() {
                                    Some(i) => {
                                        if i == 0 {
                                            0
                                        } else {
                                            i - 1
                                        }
                                    }
                                    None => 0,
                                };
                                self.table_state.select(Some(i));
                            }
                        }
                        Action::MoveMiddle => {
                            let max = self.current_row_count();
                            if max > 0 {
                                self.table_state.select(Some(max / 2));
                            }
                        }
                        Action::MoveBottom => {
                            let max = self.current_row_count();
                            if max > 0 {
                                self.table_state.select(Some(max - 1));
                            }
                        }
                        Action::MoveTop => {
                            let max = self.current_row_count();
                            if max > 0 {
                                self.table_state.select(Some(0));
                            }
                        }
                        Action::MoveEnd => {
                            let max = self.current_row_count();
                            if max > 0 {
                                self.table_state.select(Some(max - 1));
                            }
                        }
                        Action::MoveStart => {
                            let max = self.current_row_count();
                            if max > 0 {
                                self.table_state.select(Some(0));
                            }
                        }
                        // Other actions not yet fully implemented in UI logic
                        _ => {}
                    }
                }
            }
        }
    }

    fn get_account_name(&self, id: &AccountId) -> String {
        self.ledger
            .accounts
            .iter()
            .find(|a| a.id == *id)
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Unknown".to_string())
    }

    fn get_selected_account(&self) -> Option<AccountId> {
        if self.tab_index == 0 {
            let query = self.search_query.to_lowercase();
            let mut iter = self
                .ledger
                .accounts
                .iter()
                .filter(|a| self.account_matches_query(a, &query));
            if let Some(i) = self.table_state.selected() {
                iter.nth(i).map(|a| a.id)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn get_selected_transaction(&self) -> Option<gnucash_engine::domain::TransactionId> {
        if self.tab_index > 0 {
            let active_id = self.active_accounts[self.tab_index - 1];
            let query = self.search_query.to_lowercase();
            let mut iter = self
                .ledger
                .transactions
                .iter()
                .filter(|tx| tx.splits().iter().any(|s| s.account_id == active_id))
                .filter(|tx| query.is_empty() || tx.description().to_lowercase().contains(&query));

            if let Some(i) = self.table_state.selected() {
                iter.nth(i).map(|tx| tx.id())
            } else {
                None
            }
        } else {
            None
        }
    }

    fn open_selected_entry(&mut self) {
        if self.tab_index == 0 {
            if let Some(acc_id) = self.get_selected_account() {
                if let Some(index) = self.active_accounts.iter().position(|id| *id == acc_id) {
                    self.tab_index = index + 1;
                    self.table_state.select(Some(0));
                }
            }
        } else {
            // TODO: Implement transaction view
        }
    }
}

pub fn key_to_string(key: &KeyEvent) -> String {
    let mut s = String::new();

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        s.push_str("ctrl-");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        s.push_str("alt-");
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        if !matches!(key.code, KeyCode::Char(_)) {
            s.push_str("shift-");
        }
    }

    match key.code {
        KeyCode::Char(c) => s.push(c),
        KeyCode::Enter => s.push_str("enter"),
        KeyCode::Esc => s.push_str("esc"),
        KeyCode::Tab => s.push_str("tab"),
        KeyCode::BackTab => s.push_str("backtab"),
        KeyCode::Backspace => s.push_str("backspace"),
        KeyCode::Up => s.push_str("up"),
        KeyCode::Down => s.push_str("down"),
        KeyCode::Left => s.push_str("left"),
        KeyCode::Right => s.push_str("right"),
        KeyCode::F(n) => s.push_str(&format!("f{}", n)),
        _ => s.push_str("unknown"),
    }

    s
}

/// Main async TUI loop utilizing Tokio MPSC channels for non-blocking IO.
pub async fn run(ledger: Ledger, settings: AppSettings) -> Result<(), AppError> {
    // Setup terminal
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    // Create an async event channel
    let (tx, mut rx) = mpsc::channel(100);
    let tick_rate = Duration::from_millis(250);

    // Spawn a separate async task to listen to keyboard events without blocking the render loop
    tokio::spawn(async move {
        let mut last_tick = Instant::now();
        loop {
            // Calculate how much time is left until the next tick
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or(Duration::from_secs(0));

            // Poll for crossterm events with the remaining time
            if event::poll(timeout).unwrap_or(false) {
                if let Ok(CrosstermEvent::Key(key)) = event::read() {
                    if tx.send(Event::Key(key)).await.is_err() {
                        break;
                    }
                }
            }

            // If enough time has passed, send a tick event and reset the timer
            if last_tick.elapsed() >= tick_rate {
                if tx.send(Event::Tick).await.is_err() {
                    break;
                }
                last_tick = Instant::now();
            }
        }
    });

    let mut app = App::new(ledger, settings.build_key_map());

    // Main render loop
    loop {
        terminal.draw(|f| {
            let size = f.size();

            let search_height = if app.state == AppState::Search || !app.search_query.is_empty() {
                3
            } else {
                0
            };

            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),             // Title
                    Constraint::Length(3),             // Tabs
                    Constraint::Length(search_height), // Search Prompt
                    Constraint::Min(0),                // Content
                    Constraint::Length(1),             // Footer
                ])
                .split(size);

            let title = Paragraph::new("Gcash Interactive").alignment(Alignment::Center);
            f.render_widget(title, layout[0]);

            let mut all_tab_titles = vec!["Accounts Overview".to_string()];
            all_tab_titles.extend(
                app.active_accounts
                    .iter()
                    .map(|id| app.get_account_name(id)),
            );

            // Determine visible window of tabs based on available width
            let available_width = layout[1].width as usize - 2;
            if app.tab_index < app.tab_scroll_offset {
                app.tab_scroll_offset = app.tab_index;
            }

            let mut visible_titles = Vec::new();
            loop {
                visible_titles.clear();
                let mut current_width = 0;
                let mut selected_fits = false;

                for (i, title) in all_tab_titles.iter().enumerate().skip(app.tab_scroll_offset) {
                    // Estimated width: title length + divider (typically " | " which is 3 chars)
                    let title_width = title.chars().count() + 3;
                    if current_width + title_width > available_width {
                        break;
                    }
                    visible_titles.push(title.clone());
                    current_width += title_width;
                    if i == app.tab_index {
                        selected_fits = true;
                    }
                }

                if selected_fits || app.tab_scroll_offset >= app.tab_index {
                    break;
                }
                app.tab_scroll_offset += 1;
            }

            let tabs = Tabs::new(visible_titles)
                .block(Block::default().borders(Borders::ALL).title("Accounts"))
                .select(app.tab_index - app.tab_scroll_offset)
                .highlight_style(
                    ratatui::style::Style::default()
                        .add_modifier(ratatui::style::Modifier::BOLD)
                        .fg(ratatui::style::Color::Yellow),
                );
            f.render_widget(tabs, layout[1]);

            if search_height > 0 {
                let border_style = if app.state == AppState::Search {
                    ratatui::style::Style::default().fg(ratatui::style::Color::Yellow)
                } else {
                    ratatui::style::Style::default()
                };
                let search_block = Paragraph::new(app.search_query.clone()).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Search")
                        .border_style(border_style),
                );
                f.render_widget(search_block, layout[2]);
            }

            match app.state {
                AppState::View | AppState::Search => {
                    let query = app.search_query.to_lowercase();
                    if app.tab_index == 0 {
                        // Render Accounts Overview
                        let rows: Vec<Row> = app
                            .ledger
                            .accounts
                            .iter()
                            .filter(|a| app.account_matches_query(a, &query))
                            .map(|acc| {
                                Row::new(vec![
                                    Cell::from(acc.name.clone()),
                                    Cell::from(format!("{:?}", acc.account_type)),
                                    Cell::from(format!("{:?}", acc.id)),
                                ])
                            })
                            .collect();

                        let table = Table::new(
                            rows,
                            [
                                Constraint::Percentage(40),
                                Constraint::Percentage(20),
                                Constraint::Percentage(40),
                            ],
                        )
                        .header(
                            Row::new(vec!["Name", "Type", "ID"]).style(
                                ratatui::style::Style::default()
                                    .add_modifier(ratatui::style::Modifier::BOLD),
                            ),
                        )
                        .block(Block::default().borders(Borders::ALL).title("All Accounts"))
                        .highlight_style(
                            ratatui::style::Style::default()
                                .add_modifier(ratatui::style::Modifier::REVERSED),
                        )
                        .column_spacing(1);

                        f.render_stateful_widget(table, layout[3], &mut app.table_state);
                    } else {
                        // Render Specific Account Register
                        let active_id = app.active_accounts[app.tab_index - 1];

                        let rows: Vec<Row> = app
                            .ledger
                            .transactions
                            .iter()
                            .filter(|tx| tx.splits().iter().any(|s| s.account_id == active_id))
                            .filter(|tx| app.transaction_matches_query(tx, &active_id, &query))
                            .map(|tx| {
                                let date = tx.date().format("%Y-%m-%d").to_string();
                                let desc = tx.description().to_string();

                                let active_split = tx
                                    .splits()
                                    .iter()
                                    .find(|s| s.account_id == active_id)
                                    .unwrap();
                                let other_splits: Vec<_> = tx
                                    .splits()
                                    .iter()
                                    .filter(|s| s.account_id != active_id)
                                    .collect();

                                // Deduce transfer account
                                let transfer = if other_splits.len() == 1 {
                                    app.get_account_name(&other_splits[0].account_id)
                                } else if other_splits.len() > 1 {
                                    "-- Split --".to_string()
                                } else {
                                    "None".to_string()
                                };

                                let amount_val =
                                    format!("{:.2}", active_split.amount.to_f64().unwrap_or(0.0));

                                Row::new(vec![
                                    Cell::from(date),
                                    Cell::from(desc),
                                    Cell::from(transfer),
                                    Cell::from(amount_val),
                                ])
                            })
                            .collect();

                        let table = Table::new(
                            rows,
                            [
                                Constraint::Length(12),
                                Constraint::Min(20),
                                Constraint::Min(20),
                                Constraint::Length(10),
                            ],
                        )
                        .header(
                            Row::new(vec!["Date", "Description", "Transfer", "Amount"]).style(
                                ratatui::style::Style::default()
                                    .add_modifier(ratatui::style::Modifier::BOLD),
                            ),
                        )
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(format!("Register: {}", app.get_account_name(&active_id))),
                        )
                        .highlight_style(
                            ratatui::style::Style::default()
                                .add_modifier(ratatui::style::Modifier::REVERSED),
                        )
                        .column_spacing(1);

                        f.render_stateful_widget(table, layout[3], &mut app.table_state);
                    }
                }
                AppState::Edit => {
                    if let Some(ref edit_state) = app.edit_state {
                        let title = match edit_state.target {
                            EditTarget::Account(_) => "Edit Account",
                            EditTarget::Transaction(_) => "Edit Transaction",
                        };

                        let labels = match edit_state.target {
                            EditTarget::Account(_) => vec!["Name"],
                            EditTarget::Transaction(_) => vec!["Date (YYYY-MM-DD)", "Description", "Amount"],
                        };

                        let mut text = vec![
                            ratatui::text::Line::from("Press Enter to save, Esc to cancel. Tab/Shift-Tab to switch fields."),
                            ratatui::text::Line::from(""),
                        ];

                        let mut cursor_pos = None;

                        for (i, label) in labels.iter().enumerate() {
                            let is_active = i == edit_state.active_field_index;
                            let prefix = if is_active { "> " } else { "  " };
                            let val = &edit_state.fields[i];
                            let style = if is_active {
                                ratatui::style::Style::default()
                                    .add_modifier(ratatui::style::Modifier::BOLD)
                                    .fg(ratatui::style::Color::Yellow)
                            } else {
                                ratatui::style::Style::default()
                            };
                            text.push(ratatui::text::Line::styled(
                                format!("{}{}: {}", prefix, label, val),
                                style,
                            ));

                            if is_active {
                                let x = layout[3].x + 1 + (prefix.chars().count() + label.chars().count() + 2 + edit_state.cursor_position) as u16;
                                let y = layout[3].y + 1 + 2 + i as u16;
                                cursor_pos = Some((x, y));
                            }
                        }

                        let content = Paragraph::new(text)
                            .block(Block::default().borders(Borders::ALL).title(title))
                            .alignment(Alignment::Left);
                        f.render_widget(content, layout[3]);

                        if let Some((x, y)) = cursor_pos {
                            f.set_cursor(x, y);
                        }
                    } else {
                        let content = Paragraph::new("No item selected for editing.")
                            .block(Block::default().borders(Borders::ALL))
                            .alignment(Alignment::Center);
                        f.render_widget(content, layout[3]);
                    }
                }
                AppState::Help => {
                    let mut action_map: HashMap<Action, Vec<String>> = HashMap::new();
                    for (key, action) in &app.key_map {
                        action_map.entry(*action).or_default().push(key.clone());
                    }

                    let mut rows = Vec::new();
                    let mut sorted_actions: Vec<_> = action_map.keys().collect();
                    sorted_actions.sort_by_key(|a| format!("{:?}", a));

                    for action in sorted_actions {
                        if let Some(keys) = action_map.get(action) {
                            let mut sorted_keys = keys.clone();
                            sorted_keys.sort();
                            rows.push(Row::new(vec![
                                Cell::from(format!("{:?}", action)),
                                Cell::from(sorted_keys.join(", ")),
                            ]));
                        }
                    }

                    let table = Table::new(
                        rows,
                        [
                            Constraint::Percentage(40),
                            Constraint::Percentage(60),
                        ],
                    )
                    .header(
                        Row::new(vec!["Action", "Keys"]).style(
                            ratatui::style::Style::default()
                                .add_modifier(ratatui::style::Modifier::BOLD),
                        ),
                    )
                    .block(Block::default().borders(Borders::ALL).title("Keyboard Shortcuts Cheat Sheet"))
                    .column_spacing(2);

                    f.render_widget(table, layout[3]);
                }
            };

            let footer = Paragraph::new("Press '?' for help | 'q' to quit")
                .alignment(Alignment::Left);
            f.render_widget(footer, layout[4]);
        })?;

        // Handle async events from our channel
        if let Some(event) = rx.recv().await {
            app.update(event);
        }

        if app.should_quit {
            break;
        }
    }

    // Teardown terminal
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use gnucash_engine::domain::types::CommodityId;
    use gnucash_engine::domain::{Account, AccountType, DraftTransaction, Ledger, Split};
    use num_rational::Rational64;

    fn make_key_event(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        })
    }

    fn create_test_ledger() -> Ledger {
        let commodity = CommodityId::new("USD");
        let account1 = Account::new("Checking", AccountType::Bank, commodity.clone());
        let account2 = Account::new("Groceries", AccountType::Expense, commodity.clone());
        let account3 = Account::new("Dining", AccountType::Expense, commodity.clone());

        let split1 = Split::new(account1.id, Rational64::new(-100, 1));
        let split2 = Split::new(account2.id, Rational64::new(100, 1));

        let tx1 = DraftTransaction::new(commodity.clone())
            .with_description("Walmart")
            .add_split(split1)
            .add_split(split2)
            .validate()
            .unwrap();

        let split3 = Split::new(account1.id, Rational64::new(-15, 1));
        let split4 = Split::new(account3.id, Rational64::new(15, 1));

        let tx2 = DraftTransaction::new(commodity)
            .with_description("Jimmy John's")
            .add_split(split3)
            .add_split(split4)
            .validate()
            .unwrap();

        Ledger::new(vec![account1, account2, account3], vec![tx1, tx2])
    }

    fn get_app_with_defaults() -> App {
        let ledger = create_test_ledger();

        let settings = AppSettings {
            database_url: None,
            keybindings: AppSettings::default_keybindings(),
        };

        App::new(ledger, settings.build_key_map())
    }

    #[test]
    fn test_quit_action() {
        let mut app = get_app_with_defaults();
        assert!(!app.should_quit);

        app.update(make_key_event(KeyCode::Char('q'), KeyModifiers::empty()));
        assert!(app.should_quit);
    }

    #[test]
    fn test_quit_action_esc() {
        let mut app = get_app_with_defaults();
        assert!(!app.should_quit);

        app.update(make_key_event(KeyCode::Esc, KeyModifiers::empty()));
        assert!(app.should_quit);
    }

    #[test]
    fn test_view_mode_action() {
        let mut app = get_app_with_defaults();
        app.state = AppState::Edit; // Start in Edit mode
        app.edit_state = Some(EditState {
            target: EditTarget::Account(app.active_accounts[0]),
            fields: vec!["test".to_string()],
            active_field_index: 0,
            cursor_position: 4,
        });

        app.update(make_key_event(KeyCode::Esc, KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::View));
        assert!(app.edit_state.is_none());
    }

    #[test]
    fn test_search_action_and_typing() {
        let mut app = get_app_with_defaults();
        assert!(matches!(app.state, AppState::View));
        assert!(app.search_query.is_empty());
        assert_eq!(app.current_row_count(), 3); // Checking, Groceries, Dining

        // Pressing '/' should enter search mode
        app.update(make_key_event(KeyCode::Char('/'), KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::Search));

        // Typing characters should append to search_query
        app.update(make_key_event(KeyCode::Char('c'), KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('h'), KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('e'), KeyModifiers::empty()));
        assert_eq!(app.search_query, "che");

        // The query "che" should match "Checking".
        assert_eq!(app.current_row_count(), 1);

        // Backspace should remove characters
        app.update(make_key_event(KeyCode::Backspace, KeyModifiers::empty()));
        assert_eq!(app.search_query, "ch");
        assert_eq!(app.current_row_count(), 1);

        // Pressing Enter should apply, exit search mode, AND navigate to the account tab
        app.update(make_key_event(KeyCode::Enter, KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::View));
        assert_eq!(app.search_query, "ch"); // Query should remain
        assert_eq!(app.tab_index, 1); // Navigated to Checking

        // Clear search for next part of test
        app.update(make_key_event(KeyCode::Char('/'), KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Esc, KeyModifiers::empty()));
        assert_eq!(app.current_row_count(), 2); // Walmart, Jimmy John's in Checking

        // Go back to overview
        app.tab_index = 0;
        assert_eq!(app.current_row_count(), 3);

        // Switch to "Checking" tab
        app.update(make_key_event(KeyCode::Tab, KeyModifiers::empty()));
        assert_eq!(app.tab_index, 1);
        assert_eq!(app.current_row_count(), 2); // Walmart, Jimmy John's

        // Search for "wal"
        app.update(make_key_event(KeyCode::Char('/'), KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('w'), KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('a'), KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('l'), KeyModifiers::empty()));
        assert_eq!(app.current_row_count(), 1); // Only Walmart

        // Search for "15.00" (Amount)
        app.update(make_key_event(KeyCode::Backspace, KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Backspace, KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Backspace, KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('1'), KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('5'), KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('.'), KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('0'), KeyModifiers::empty()));
        assert_eq!(app.current_row_count(), 1); // Only Jimmy John's ($15.00)
    }

    #[test]
    fn test_enter_in_search_opens_account() {
        let mut app = get_app_with_defaults();
        app.update(make_key_event(KeyCode::Char('/'), KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('c'), KeyModifiers::empty())); // "che"
        app.update(make_key_event(KeyCode::Char('h'), KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('e'), KeyModifiers::empty()));

        // Selection should be on "Checking" (index 0 of filtered list)
        assert_eq!(app.table_state.selected(), Some(0));

        // Press Enter
        app.update(make_key_event(KeyCode::Enter, KeyModifiers::empty()));

        // Should exit search AND navigate to Checking tab (index 1)
        assert_eq!(app.tab_index, 1);
    }

    #[test]
    fn test_search_action_cancel() {
        let mut app = get_app_with_defaults();

        // Enter search mode
        app.update(make_key_event(KeyCode::Char('/'), KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::Search));

        // Type
        app.update(make_key_event(KeyCode::Char('x'), KeyModifiers::empty()));
        assert_eq!(app.search_query, "x");

        // Pressing Esc should clear and exit
        app.update(make_key_event(KeyCode::Esc, KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::View));
        assert!(app.search_query.is_empty());
    }

    #[test]
    fn test_edit_account_name() {
        let mut app = get_app_with_defaults();
        assert_eq!(app.tab_index, 0); // Overview
        app.table_state.select(Some(0)); // Select "Checking"

        // Press 'i' to edit
        app.update(make_key_event(KeyCode::Char('i'), KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::Edit));

        if let Some(edit_state) = &app.edit_state {
            assert_eq!(edit_state.fields[0], "Checking");
        } else {
            panic!("Expected edit state to be populated");
        }

        // Add " Bank" to the name
        app.update(make_key_event(KeyCode::Char(' '), KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('B'), KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('a'), KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('n'), KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('k'), KeyModifiers::empty()));

        // Press Enter to save
        app.update(make_key_event(KeyCode::Enter, KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::View));
        assert!(app.edit_state.is_none());

        assert_eq!(app.ledger.accounts[0].name, "Checking Bank");
    }

    #[test]
    fn test_edit_cursor_navigation() {
        let mut app = get_app_with_defaults();
        app.tab_index = 0;
        app.table_state.select(Some(0)); // Select "Checking"

        // Enter edit mode
        app.update(make_key_event(KeyCode::Char('i'), KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::Edit));

        if let Some(edit_state) = &app.edit_state {
            assert_eq!(edit_state.fields[0], "Checking");
            assert_eq!(edit_state.cursor_position, 8); // end of string
        } else {
            panic!("Expected edit state");
        }

        // Move left 3 times
        app.update(make_key_event(KeyCode::Left, KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Left, KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Left, KeyModifiers::empty()));

        if let Some(edit_state) = &app.edit_state {
            assert_eq!(edit_state.cursor_position, 5); // between 'k' and 'i'
        }

        // Insert 'x'
        app.update(make_key_event(KeyCode::Char('x'), KeyModifiers::empty()));

        if let Some(edit_state) = &app.edit_state {
            assert_eq!(edit_state.fields[0], "Checkxing");
            assert_eq!(edit_state.cursor_position, 6);
        }

        // Backspace
        app.update(make_key_event(KeyCode::Backspace, KeyModifiers::empty()));

        if let Some(edit_state) = &app.edit_state {
            assert_eq!(edit_state.fields[0], "Checking");
            assert_eq!(edit_state.cursor_position, 5);
        }

        // Delete
        app.update(make_key_event(KeyCode::Delete, KeyModifiers::empty()));

        if let Some(edit_state) = &app.edit_state {
            assert_eq!(edit_state.fields[0], "Checkng");
            assert_eq!(edit_state.cursor_position, 5);
        }

        // Move Right
        app.update(make_key_event(KeyCode::Right, KeyModifiers::empty()));
        if let Some(edit_state) = &app.edit_state {
            assert_eq!(edit_state.cursor_position, 6);
        }
    }

    #[test]
    fn test_edit_transaction_description() {
        let mut app = get_app_with_defaults();
        // Go to Checking account tab
        app.update(make_key_event(KeyCode::Tab, KeyModifiers::empty()));
        assert_eq!(app.tab_index, 1);
        app.table_state.select(Some(0)); // Select first transaction ("Walmart")

        // Press 'i' to edit
        app.update(make_key_event(KeyCode::Char('i'), KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::Edit));

        if let Some(edit_state) = &app.edit_state {
            assert_eq!(edit_state.fields[1], "Walmart");
            assert_eq!(edit_state.active_field_index, 0); // starts at Date
        } else {
            panic!("Expected edit state to be populated");
        }

        // Tab to Description field
        app.update(make_key_event(KeyCode::Tab, KeyModifiers::empty()));
        if let Some(edit_state) = &app.edit_state {
            assert_eq!(edit_state.active_field_index, 1);
        }

        // Backspace to delete 't' then add "mart" to make it "Walmarmart" (just something different)
        app.update(make_key_event(KeyCode::Backspace, KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('s'), KeyModifiers::empty()));

        // Press Enter to save
        app.update(make_key_event(KeyCode::Enter, KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::View));

        assert_eq!(app.ledger.transactions[0].description(), "Walmars");
    }

    #[test]
    fn test_edit_transaction_amount() {
        let mut app = get_app_with_defaults();
        // Go to Checking account tab
        app.update(make_key_event(KeyCode::Tab, KeyModifiers::empty()));
        assert_eq!(app.tab_index, 1);
        app.table_state.select(Some(0)); // Select first transaction ("Walmart")

        // Press 'i' to edit
        app.update(make_key_event(KeyCode::Char('i'), KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::Edit));

        if let Some(edit_state) = &app.edit_state {
            assert_eq!(edit_state.fields[2], "-100.00");
        } else {
            panic!("Expected edit state to be populated");
        }

        // Tab twice to Amount field
        app.update(make_key_event(KeyCode::Tab, KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Tab, KeyModifiers::empty()));
        if let Some(edit_state) = &app.edit_state {
            assert_eq!(edit_state.active_field_index, 2);
        }

        // Clear existing amount
        for _ in 0..7 {
            app.update(make_key_event(KeyCode::Backspace, KeyModifiers::empty()));
        }

        // Type new amount -123.45
        for c in "-123.45".chars() {
            app.update(make_key_event(KeyCode::Char(c), KeyModifiers::empty()));
        }

        // Press Enter to save
        app.update(make_key_event(KeyCode::Enter, KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::View));

        let tx = &app.ledger.transactions[0];
        let active_id = app.active_accounts[0]; // Checking account
        let active_split = tx
            .splits()
            .iter()
            .find(|s| s.account_id == active_id)
            .unwrap();
        assert_eq!(active_split.amount, Rational64::new(-12345, 100));

        let other_split = tx
            .splits()
            .iter()
            .find(|s| s.account_id != active_id)
            .unwrap();
        assert_eq!(other_split.amount, Rational64::new(12345, 100));
    }

    #[test]
    fn test_edit_mode_action() {
        let mut app = get_app_with_defaults();
        assert!(matches!(app.state, AppState::View)); // Start in View mode
        app.table_state.select(Some(0)); // Select an account

        // 'i' should trigger EditEntry
        app.update(make_key_event(KeyCode::Char('i'), KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::Edit));
        assert!(app.edit_state.is_some());
    }

    #[test]
    fn test_open_account_action() {
        let mut app = get_app_with_defaults();
        assert_eq!(app.tab_index, 0); // Start in Overview mode
        app.table_state.select(Some(0)); // Select first account ("Checking")

        // Press 'enter' to open
        app.update(make_key_event(KeyCode::Enter, KeyModifiers::empty()));

        // Checking is active_accounts[0], so it should set tab_index = 0 + 1 = 1
        assert_eq!(app.tab_index, 1);
        assert_eq!(app.table_state.selected(), Some(0));
    }

    #[test]
    fn test_move_right_action() {
        let mut app = get_app_with_defaults();
        assert_eq!(app.tab_index, 0);

        // 'l' should move right
        app.update(make_key_event(KeyCode::Char('l'), KeyModifiers::empty()));
        assert_eq!(app.tab_index, 1);

        // 'right' should move right
        app.update(make_key_event(KeyCode::Right, KeyModifiers::empty()));
        assert_eq!(app.tab_index, 2);

        // 'tab' should move right (FocusNext) twice to wrap around
        app.update(make_key_event(KeyCode::Tab, KeyModifiers::empty()));
        assert_eq!(app.tab_index, 3);
        app.update(make_key_event(KeyCode::Tab, KeyModifiers::empty()));
        assert_eq!(app.tab_index, 0); // Wraps around (3 active accounts + 1 overview = 4 tabs)
    }

    #[test]
    fn test_move_left_action() {
        let mut app = get_app_with_defaults();
        assert_eq!(app.tab_index, 0);

        // 'h' should move left (wrapping around to 3)
        app.update(make_key_event(KeyCode::Char('h'), KeyModifiers::empty()));
        assert_eq!(app.tab_index, 3);

        // 'left' should move left
        app.update(make_key_event(KeyCode::Left, KeyModifiers::empty()));
        assert_eq!(app.tab_index, 2);

        // 'backtab' should move left (FocusPrev)
        app.update(make_key_event(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(app.tab_index, 1);
    }

    #[test]
    fn test_move_down_action() {
        let mut app = get_app_with_defaults();
        assert_eq!(app.table_state.selected(), Some(0));

        // Total accounts in test_ledger is 3
        app.update(make_key_event(KeyCode::Char('j'), KeyModifiers::empty()));
        assert_eq!(app.table_state.selected(), Some(1));
        app.update(make_key_event(KeyCode::Char('j'), KeyModifiers::empty()));
        assert_eq!(app.table_state.selected(), Some(2));

        // Shouldn't go past max bounds (max is 2, length is 3)
        app.update(make_key_event(KeyCode::Char('j'), KeyModifiers::empty()));
        assert_eq!(app.table_state.selected(), Some(2));
    }

    #[test]
    fn test_move_up_action() {
        let mut app = get_app_with_defaults();

        // Setup state to be at the bottom
        app.table_state.select(Some(1));

        app.update(make_key_event(KeyCode::Char('k'), KeyModifiers::empty()));
        assert_eq!(app.table_state.selected(), Some(0));

        // Shouldn't go past 0
        app.update(make_key_event(KeyCode::Char('k'), KeyModifiers::empty()));
        assert_eq!(app.table_state.selected(), Some(0));
    }

    #[test]
    fn test_move_middle_action() {
        let mut app = get_app_with_defaults();
        assert_eq!(app.current_row_count(), 3);

        // Press 'M' to move to middle
        app.update(make_key_event(KeyCode::Char('M'), KeyModifiers::SHIFT));

        // max = 3. 3 / 2 = 1.
        assert_eq!(app.table_state.selected(), Some(1));
    }

    #[test]
    fn test_move_bottom_action() {
        let mut app = get_app_with_defaults();
        assert_eq!(app.current_row_count(), 3);

        // Press 'L' to move to bottom
        app.update(make_key_event(KeyCode::Char('L'), KeyModifiers::SHIFT));

        // max = 3. select index 2.
        assert_eq!(app.table_state.selected(), Some(2));
    }

    #[test]
    fn test_move_top_action() {
        let mut app = get_app_with_defaults();
        assert_eq!(app.current_row_count(), 3);

        // First move down so we aren't already at the top
        app.table_state.select(Some(2));

        // Press 'H' to move to top
        app.update(make_key_event(KeyCode::Char('H'), KeyModifiers::SHIFT));

        assert_eq!(app.table_state.selected(), Some(0));
    }

    #[test]
    fn test_move_end_action() {
        let mut app = get_app_with_defaults();
        assert_eq!(app.current_row_count(), 3);

        // Press 'G' to move to end of list
        app.update(make_key_event(KeyCode::Char('G'), KeyModifiers::SHIFT));

        assert_eq!(app.table_state.selected(), Some(2));
    }

    #[test]
    fn test_move_start_action() {
        let mut app = get_app_with_defaults();
        assert_eq!(app.current_row_count(), 3);

        // First move down
        app.table_state.select(Some(2));

        // Press 'g' then 'g'
        app.update(make_key_event(KeyCode::Char('g'), KeyModifiers::empty()));
        assert_eq!(app.pending_keys, "g");
        app.update(make_key_event(KeyCode::Char('g'), KeyModifiers::empty()));
        assert_eq!(app.pending_keys, "");

        assert_eq!(app.table_state.selected(), Some(0));
    }

    #[test]
    fn test_show_help_action() {
        let mut app = get_app_with_defaults();
        assert!(matches!(app.state, AppState::View));

        // Pressing '?' should enter help mode
        app.update(make_key_event(KeyCode::Char('?'), KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::Help));

        // Pressing Esc should return to View
        app.update(make_key_event(KeyCode::Esc, KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::View));

        // Re-enter help
        app.update(make_key_event(KeyCode::Char('?'), KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::Help));

        // Pressing 'q' should return to View (not quit)
        app.update(make_key_event(KeyCode::Char('q'), KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::View));
        assert!(!app.should_quit);
    }

    #[test]
    fn test_key_to_string() {
        // Character keys
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Char('q'), KeyModifiers::empty()).into_key()),
            "q"
        );
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Char('h'), KeyModifiers::empty()).into_key()),
            "h"
        );
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Char('j'), KeyModifiers::empty()).into_key()),
            "j"
        );
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Char('k'), KeyModifiers::empty()).into_key()),
            "k"
        );
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Char('l'), KeyModifiers::empty()).into_key()),
            "l"
        );
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Char(' '), KeyModifiers::empty()).into_key()),
            " "
        );
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Char('/'), KeyModifiers::empty()).into_key()),
            "/"
        );

        // Case sensitivity
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Char('m'), KeyModifiers::empty()).into_key()),
            "m"
        );
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Char('M'), KeyModifiers::SHIFT).into_key()),
            "M"
        );

        // Control characters
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Char('f'), KeyModifiers::CONTROL).into_key()),
            "ctrl-f"
        );
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL).into_key()),
            "ctrl-c"
        );

        // Special keys
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Enter, KeyModifiers::empty()).into_key()),
            "enter"
        );
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Esc, KeyModifiers::empty()).into_key()),
            "esc"
        );
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Tab, KeyModifiers::empty()).into_key()),
            "tab"
        );
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::BackTab, KeyModifiers::SHIFT).into_key()),
            "shift-backtab"
        );
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Up, KeyModifiers::empty()).into_key()),
            "up"
        );
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Down, KeyModifiers::empty()).into_key()),
            "down"
        );
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Left, KeyModifiers::empty()).into_key()),
            "left"
        );
        assert_eq!(
            key_to_string(&make_key_event(KeyCode::Right, KeyModifiers::empty()).into_key()),
            "right"
        );
    }

    impl Event {
        fn into_key(self) -> KeyEvent {
            if let Event::Key(k) = self {
                k
            } else {
                unreachable!()
            }
        }
    }
}
