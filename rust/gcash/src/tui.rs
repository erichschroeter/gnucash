use crossterm::{
    event::{self, Event as CrosstermEvent, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    widgets::{Block, Borders, Paragraph, Table, Row, Cell},
    Terminal,
};
use std::{io, time::Duration};
use tokio::sync::mpsc;
use crate::error::AppError;
use gnucash_engine::domain::Ledger;
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
}

impl App {
    pub fn new(ledger: Ledger) -> Self {
        Self {
            state: AppState::View,
            should_quit: false,
            ledger,
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
                    _ => {}
                }
            }
        }
    }

    fn get_account_name(&self, id: &gnucash_engine::domain::AccountId) -> String {
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
                    Constraint::Length(3), // Title
                    Constraint::Min(0),    // Content
                    Constraint::Length(1), // Footer
                ])
                .split(size);

            let title_block = Block::default().title("Gcash TUI").borders(Borders::ALL);
            let title = Paragraph::new("Transactions Register")
                .block(title_block)
                .alignment(Alignment::Center);
            f.render_widget(title, layout[0]);

            match app.state {
                AppState::View => {
                    let rows: Vec<Row> = app.ledger.transactions.iter().map(|tx| {
                        let date = tx.date().format("%Y-%m-%d").to_string();
                        let desc = tx.description().to_string();
                        
                        // Deduce transfer account
                        let transfer = if tx.splits().len() == 2 {
                            // Simple transaction, show the OTHER account
                            app.get_account_name(&tx.splits()[1].account_id)
                        } else if tx.splits().len() > 2 {
                            "-- Split --".to_string()
                        } else {
                            "None".to_string()
                        };

                        let amount_val = if !tx.splits().is_empty() {
                            format!("{:.2}", tx.splits()[0].amount.to_f64().unwrap_or(0.0))
                        } else {
                            "0.00".to_string()
                        };

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
                    .block(Block::default().borders(Borders::ALL).title("Transactions"))
                    .column_spacing(1);

                    f.render_widget(table, layout[1]);
                }
                AppState::Edit => {
                    let content = Paragraph::new("Edit Mode: Feature not yet implemented.\nPress 'v' to return to View.")
                        .block(Block::default().borders(Borders::ALL))
                        .alignment(Alignment::Center);
                    f.render_widget(content, layout[1]);
                }
            };

            let footer = Paragraph::new("Press 'v' for View, 'e' for Edit, 'q' to Quit")
                .alignment(Alignment::Left);
            f.render_widget(footer, layout[2]);
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
