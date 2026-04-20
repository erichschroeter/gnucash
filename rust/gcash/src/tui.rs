use crate::config::{Action, AppSettings};
use crate::error::AppError;
use crossterm::{
    event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use gnucash_engine::domain::{AccountId, Ledger};
use num_traits::ToPrimitive;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Tabs},
    Terminal,
};
use std::{collections::HashMap, io, time::{Duration, Instant}};
use tokio::sync::mpsc;

/// TUI events handled by the async event loop.
pub enum Event {
    Tick,
    Key(event::KeyEvent),
}

/// Application view states for Model-View-Update architecture.
#[derive(PartialEq, Eq)]
pub enum AppState {
    View,
    Edit,
    Search,
}

/// The core Model for the interactive application.
pub struct App {
    pub state: AppState,
    pub should_quit: bool,
    pub ledger: Ledger,
    pub tab_index: usize,
    pub active_accounts: Vec<AccountId>,
    pub key_map: HashMap<String, Action>,
    pub table_state: ratatui::widgets::TableState,
    pub search_query: String,
}

impl App {
    pub fn new(ledger: Ledger, key_map: HashMap<String, Action>) -> Self {
        let mut active_accounts = std::collections::HashSet::new();
        for tx in &ledger.transactions {
            for split in tx.splits() {
                active_accounts.insert(split.account_id);
            }
        }
        let mut active_accounts: Vec<AccountId> = active_accounts.into_iter().collect();
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
            should_quit: false,
            ledger,
            tab_index: 0,
            active_accounts,
            key_map,
            table_state,
            search_query: String::new(),
        }
    }

    fn account_has_matching_transaction(&self, account_id: &AccountId, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        self.ledger.transactions.iter()
            .filter(|tx| tx.splits().iter().any(|s| s.account_id == *account_id))
            .any(|tx| tx.description().to_lowercase().contains(query))
    }

    fn current_row_count(&self) -> usize {
        let query = self.search_query.to_lowercase();
        if self.tab_index == 0 {
            self.ledger.accounts.iter()
                .filter(|a| self.account_has_matching_transaction(&a.id, &query))
                .count()
        } else {
            let active_id = self.active_accounts[self.tab_index - 1];
            self.ledger.transactions.iter()
                .filter(|tx| tx.splits().iter().any(|s| s.account_id == active_id))
                .filter(|tx| query.is_empty() || tx.description().to_lowercase().contains(&query))
                .count()
        }
    }

    /// Update logic applied when a new event is received.
    pub fn update(&mut self, event: Event) {
        if let Event::Key(key) = event {
            // Only trigger on key press down
            if key.kind == KeyEventKind::Press {
                // If in search mode, capture raw typing instead of action map
                if self.state == AppState::Search {
                    match key.code {
                        KeyCode::Char(c) => self.search_query.push(c),
                        KeyCode::Backspace => {
                            self.search_query.pop();
                        }
                        KeyCode::Enter => self.state = AppState::View,
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

                let key_str = key_to_string(&key);
                if let Some(action) = self.key_map.get(&key_str) {
                    match action {
                        Action::Quit => self.should_quit = true,
                        Action::ViewMode => self.state = AppState::View,
                        Action::EditMode => self.state = AppState::Edit,
                        Action::Search => self.state = AppState::Search,
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
        KeyCode::Char(c) => s.push(c.to_ascii_lowercase()),
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

            let mut tab_titles = vec!["Accounts Overview".to_string()];
            tab_titles.extend(
                app.active_accounts
                    .iter()
                    .map(|id| app.get_account_name(id)),
            );

            let tabs = Tabs::new(tab_titles)
                .block(Block::default().borders(Borders::ALL).title("Accounts"))
                .select(app.tab_index)
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
                            .filter(|a| app.account_has_matching_transaction(&a.id, &query))
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
                            .filter(|tx| {
                                query.is_empty() || tx.description().to_lowercase().contains(&query)
                            })
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
                    let content = Paragraph::new(
                        "Edit Mode: Feature not yet implemented.\nPress 'v' to return to View.",
                    )
                    .block(Block::default().borders(Borders::ALL))
                    .alignment(Alignment::Center);
                    f.render_widget(content, layout[3]);
                }
            };

            let footer = Paragraph::new(
                "Arrows/HJKL/Tab: Navigate | '/': Search | 'v': View | 'e': Edit | 'q': Quit",
            )
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

        app.update(make_key_event(KeyCode::Char('v'), KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::View));
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
        app.update(make_key_event(KeyCode::Char('j'), KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('i'), KeyModifiers::empty()));
        app.update(make_key_event(KeyCode::Char('m'), KeyModifiers::empty()));
        assert_eq!(app.search_query, "jim");

        // The query "jim" should match Jimmy John's, which is associated with Checking and Dining.
        // Groceries should be filtered out.
        assert_eq!(app.current_row_count(), 2);

        // Backspace should remove characters
        app.update(make_key_event(KeyCode::Backspace, KeyModifiers::empty()));
        assert_eq!(app.search_query, "ji");
        assert_eq!(app.current_row_count(), 2);

        // Pressing Enter should apply and exit search mode
        app.update(make_key_event(KeyCode::Enter, KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::View));
        assert_eq!(app.search_query, "ji"); // Query should remain
        assert_eq!(app.current_row_count(), 2);
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
    fn test_edit_mode_action() {
        let mut app = get_app_with_defaults();
        assert!(matches!(app.state, AppState::View)); // Start in View mode

        app.update(make_key_event(KeyCode::Char('e'), KeyModifiers::empty()));
        assert!(matches!(app.state, AppState::Edit));
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
