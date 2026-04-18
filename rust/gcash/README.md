# gcash

A robust asynchronous Rust CLI application with an interactive TUI built using Ratatui and Crossterm.

## Usage

Run in foreground mode:
```bash
cargo run
```

Run in interactive TUI mode:
```bash
cargo run -- --interactive
```

Specify a custom config and verbosity:
```bash
cargo run -- --config custom.yml --verbosity debug --interactive
```
