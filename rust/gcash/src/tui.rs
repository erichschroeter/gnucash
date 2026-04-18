use crossterm::{
    event::{self, Event as CrosstermEvent, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    widgets::{Block, Borders, Paragraph, Table, Row, Cell, Tabs},
    Terminal,
};
use std::{io, time::Duration};
use tokio::sync::mpsc;
use crate::error::AppError;
use gnucash_engine::domain::{Ledger, AccountId};
use num_traits::ToPrimitive;

/// TUI events handled by the async event loop.
pub enum Event {
    Tick,
    Key(event::KeyEvent),
}

/// Application view states for Model-View-Update architecture.
pub enum AppState {
    View,
    Edit,
}

/// The core Model for the interactive application.
pub struct App {
    pub state: AppState,
    pub should_quit: bool,
    pub ledger: Ledger,
    pub tab_index: usize,
    pub active_accounts: Vec<AccountId>,
}

impl App {
    pub fn new(ledger: Ledger) -> Self {
        let mut active_accounts = std::collections::HashSet::new();
        for tx in &ledger.transactions {
            for split in tx.splits() {
                active_accounts.insert(split.account_id);
            }
        }
        let mut active_accounts: Vec<AccountId> = active_accounts.into_iter().collect();
        // Sort active accounts by name for consistent tab order
        active_accounts.sort_by_cached_key(|id| {
            ledger.accounts.iter()
                .find(|a| a.id == *id)
                .map(|a| a.name.clone())
                .unwrap_or_default()
        });

        Self {
            state: AppState::View,
            should_quit: false,
            ledger,
            tab_index: 0,
            active_accounts,
        }
    }

    /// Update logic applied when a new event is received.
    pub fn update(&mut self, event: Event) {
        if let Event::Key(key) = event {
            // Only trigger on key press down
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
                    KeyCode::Char('v') => self.state = AppState::View,
                    KeyCode::Char('e') => self.state = AppState::Edit,
                    KeyCode::Right | KeyCode::Tab => {
                        let total_tabs = self.active_accounts.len() + 1;
                        self.tab_index = (self.tab_index + 1) % total_tabs;
                    }
                    KeyCode::Left | KeyCode::BackTab => {
                        let total_tabs = self.active_accounts.len() + 1;
                        if self.tab_index == 0 {
                            self.tab_index = total_tabs - 1;
                        } else {
                            self.tab_index -= 1;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn get_account_name(&self, id: &AccountId) -> String {
        self.ledger.accounts.iter()
            .find(|a| a.id == *id)
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Unknown".to_string())
    }
}

/// Main async TUI loop utilizing Tokio MPSC channels for non-blocking IO.
pub async fn run(ledger: Ledger) -> Result<(), AppError> {
    // Setup terminal
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    // Create an async event channel
    let (tx, mut rx) = mpsc::channel(100);
    let tick_rate = Duration::from_millis(250);

    // Spawn a separate async task to listen to keyboard events without blocking the render loop
    tokio::spawn(async move {
        loop {
            // Poll for crossterm events
            if event::poll(Duration::from_millis(50)).unwrap_or(false) {
                if let Ok(CrosstermEvent::Key(key)) = event::read() {
                    if tx.send(Event::Key(key)).await.is_err() {
                        break;
                    }
                }
            }
            // Send tick event to enforce screen refreshes/animation even without input
            if tx.send(Event::Tick).await.is_err() {
                break;
            }
            tokio::time::sleep(tick_rate).await;
        }
    });

    let mut app = App::new(ledger);

    // Main render loop
    loop {
        terminal.draw(|f| {
            let size = f.size();
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Title
                    Constraint::Length(3), // Tabs
                    Constraint::Min(0),    // Content
                    Constraint::Length(1), // Footer
                ])
                .split(size);

            let title = Paragraph::new("Gcash Interactive").alignment(Alignment::Center);
            f.render_widget(title, layout[0]);

            let mut tab_titles = vec!["Accounts Overview".to_string()];
            tab_titles.extend(app.active_accounts.iter().map(|id| app.get_account_name(id)));
            
            let tabs = Tabs::new(tab_titles)
                .block(Block::default().borders(Borders::ALL).title("Accounts"))
                .select(app.tab_index)
                .highlight_style(ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::BOLD).fg(ratatui::style::Color::Yellow));
            f.render_widget(tabs, layout[1]);

            match app.state {
                AppState::View => {
                    if app.tab_index == 0 {
                        // Render Accounts Overview
                        let rows: Vec<Row> = app.ledger.accounts.iter().map(|acc| {
                            Row::new(vec![
                                Cell::from(acc.name.clone()),
                                Cell::from(format!("{:?}", acc.account_type)),
                                Cell::from(format!("{:?}", acc.id)),
                            ])
                        }).collect();

                        let table = Table::new(rows, [
                            Constraint::Percentage(40),
                            Constraint::Percentage(20),
                            Constraint::Percentage(40),
                        ])
                        .header(Row::new(vec!["Name", "Type", "ID"])
                            .style(ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::BOLD))
                        )
                        .block(Block::default().borders(Borders::ALL).title("All Accounts"))
                        .column_spacing(1);

                        f.render_widget(table, layout[2]);
                    } else {
                        // Render Specific Account Register
                        let active_id = app.active_accounts[app.tab_index - 1];
                        
                        let rows: Vec<Row> = app.ledger.transactions.iter()
                            .filter(|tx| tx.splits().iter().any(|s| s.account_id == active_id))
                            .map(|tx| {
                                let date = tx.date().format("%Y-%m-%d").to_string();
                                let desc = tx.description().to_string();
                                
                                let active_split = tx.splits().iter().find(|s| s.account_id == active_id).unwrap();
                                let other_splits: Vec<_> = tx.splits().iter().filter(|s| s.account_id != active_id).collect();

                                // Deduce transfer account
                                let transfer = if other_splits.len() == 1 {
                                    app.get_account_name(&other_splits[0].account_id)
                                } else if other_splits.len() > 1 {
                                    "-- Split --".to_string()
                                } else {
                                    "None".to_string()
                                };

                                let amount_val = format!("{:.2}", active_split.amount.to_f64().unwrap_or(0.0));

                                Row::new(vec![
                                    Cell::from(date),
                                    Cell::from(desc),
                                    Cell::from(transfer),
                                    Cell::from(amount_val),
                                ])
                            }).collect();

                        let table = Table::new(rows, [
                            Constraint::Length(12),
                            Constraint::Min(20),
                            Constraint::Min(20),
                            Constraint::Length(10),
                        ])
                        .header(Row::new(vec!["Date", "Description", "Transfer", "Amount"])
                            .style(ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::BOLD))
                        )
                        .block(Block::default().borders(Borders::ALL).title(format!("Register: {}", app.get_account_name(&active_id))))
                        .column_spacing(1);

                        f.render_widget(table, layout[2]);
                    }
                }
                AppState::Edit => {
                    let content = Paragraph::new("Edit Mode: Feature not yet implemented.\nPress 'v' to return to View.")
                        .block(Block::default().borders(Borders::ALL))
                        .alignment(Alignment::Center);
                    f.render_widget(content, layout[2]);
                }
            };

            let footer = Paragraph::new("Arrows/Tab: Change Tab | 'v': View | 'e': Edit | 'q': Quit")
                .alignment(Alignment::Left);
            f.render_widget(footer, layout[3]);
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
