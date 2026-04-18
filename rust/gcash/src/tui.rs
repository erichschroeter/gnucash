use crossterm::{
    event::{self, Event as CrosstermEvent, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use std::{io, time::Duration};
use tokio::sync::mpsc;
use crate::error::AppError;

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
}

impl App {
    pub fn new() -> Self {
        Self {
            state: AppState::View,
            should_quit: false,
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
}

/// Main async TUI loop utilizing Tokio MPSC channels for non-blocking IO.
pub async fn run() -> Result<(), AppError> {
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

    let mut app = App::new();

    // Main render loop
    loop {
        terminal.draw(|f| {
            let size = f.size();
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0)])
                .split(size);

            let content = match app.state {
                AppState::View => "View Mode: Press 'e' to Edit, 'q' to Quit",
                AppState::Edit => "Edit Mode: Press 'v' to View, 'q' to Quit",
            };

            let block = Block::default().title("Gcash TUI").borders(Borders::ALL);
            let paragraph = Paragraph::new(content)
                .block(block)
                .alignment(Alignment::Center);

            f.render_widget(paragraph, layout[0]);
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
