use crate::error::AppError;
use config::{Config, File, FileFormat};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Quit,
    ViewMode,
    EditMode,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    EditEntry,
    OpenEntry,
    FocusNext,
    FocusPrev,
    ToggleSplit,
    Search,
    ShowHelp,
    MoveMiddle,
    MoveBottom,
    MoveTop,
    MoveEnd,
    MoveStart,
}

/// The central application configuration.
#[derive(Debug, Deserialize, Serialize)]
pub struct AppSettings {
    pub database_url: Option<String>,
    #[serde(default = "AppSettings::default_keybindings")]
    pub keybindings: HashMap<Action, Vec<String>>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            database_url: Some("sqlite://local.db".to_string()),
            keybindings: Self::default_keybindings(),
        }
    }
}

impl AppSettings {
    pub fn default_keybindings() -> HashMap<Action, Vec<String>> {
        let mut bindings = HashMap::new();
        bindings.insert(Action::Quit, vec!["q".to_string(), "esc".to_string()]);
        bindings.insert(Action::ViewMode, vec!["v".to_string()]);
        bindings.insert(Action::EditMode, vec!["e".to_string()]);
        bindings.insert(Action::MoveLeft, vec!["h".to_string(), "left".to_string()]);
        bindings.insert(
            Action::MoveRight,
            vec!["l".to_string(), "right".to_string()],
        );
        bindings.insert(Action::MoveUp, vec!["k".to_string(), "up".to_string()]);
        bindings.insert(Action::MoveDown, vec!["j".to_string(), "down".to_string()]);
        bindings.insert(Action::EditEntry, vec!["i".to_string()]);
        bindings.insert(Action::OpenEntry, vec!["enter".to_string()]);
        bindings.insert(Action::FocusNext, vec!["tab".to_string()]);
        bindings.insert(Action::FocusPrev, vec!["shift-backtab".to_string()]);
        bindings.insert(Action::ToggleSplit, vec![" ".to_string()]);
        bindings.insert(Action::Search, vec!["/".to_string(), "ctrl-f".to_string()]);
        bindings.insert(Action::ShowHelp, vec!["?".to_string()]);
        bindings.insert(Action::MoveMiddle, vec!["M".to_string()]);
        bindings.insert(Action::MoveBottom, vec!["L".to_string()]);
        bindings.insert(Action::MoveTop, vec!["H".to_string()]);
        bindings.insert(Action::MoveEnd, vec!["G".to_string()]);
        bindings.insert(Action::MoveStart, vec!["gg".to_string()]);
        bindings
    }

    /// Flattens the config into a fast lookup map: "key_string" -> Action
    pub fn build_key_map(&self) -> HashMap<String, Action> {
        let mut map = HashMap::new();
        for (action, keys) in &self.keybindings {
            for key in keys {
                map.insert(key.clone(), *action);
            }
        }
        map
    }

    /// Returns the default configuration as a YAML string.
    pub fn default_config_yaml() -> String {
        serde_yaml::to_string(&Self::default()).unwrap_or_default()
    }
}

/// Loads configuration settings using a hierarchical fallback mechanism:
/// 1. `--config PATH` (CLI Argument - Highest priority)
/// 2. `~/.config/gcash/default.yml`
/// 3. `/etc/gcash/default.yml` (Lowest priority)
pub fn load_config(cli_config_path: Option<&String>) -> Result<AppSettings, AppError> {
    let mut builder = Config::builder();

    // 3. Lowest Priority: System-wide default
    builder =
        builder.add_source(File::new("/etc/gcash/default.yml", FileFormat::Yaml).required(false));

    // 2. Middle Priority: User-specific default
    if let Some(mut home_path) = dirs::home_dir() {
        home_path.push(".config");
        home_path.push("gcash");
        home_path.push("default.yml");
        if let Some(path_str) = home_path.to_str() {
            builder = builder.add_source(File::new(path_str, FileFormat::Yaml).required(false));
        }
    }

    // 1. Highest Priority: Provided via CLI argument
    if let Some(path) = cli_config_path {
        builder = builder.add_source(File::new(path, FileFormat::Yaml).required(true));
    }

    let config = builder.build()?;
    Ok(config.try_deserialize()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;
    use unindent::unindent;

    #[test]
    fn test_load_config_from_cli_path() {
        // Unindent cleans up the formatting of the multiline string literal
        let config_content = unindent(
            r#"
            database_url: "sqlite://test.db"
        "#,
        );

        // Tempfile ensures isolated test environments that clean themselves up
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        write!(temp_file, "{}", config_content).expect("Failed to write to temp file");

        let temp_path = temp_file.path().to_string_lossy().to_string();
        let settings = load_config(Some(&temp_path)).expect("Failed to load config");

        assert_eq!(settings.database_url, Some("sqlite://test.db".to_string()));
    }

    #[test]
    fn test_load_config_with_keybindings() {
        let config_content = unindent(
            r#"
            keybindings:
                quit: ["ctrl-c"]
                move_left: ["h", "left"]
        "#,
        );

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        write!(temp_file, "{}", config_content).expect("Failed to write to temp file");

        let temp_path = temp_file.path().to_string_lossy().to_string();
        let settings = load_config(Some(&temp_path)).expect("Failed to load config");

        let quit_bindings = settings
            .keybindings
            .get(&Action::Quit)
            .expect("Quit action not found");
        assert!(quit_bindings.contains(&"ctrl-c".to_string()));

        let move_left_bindings = settings
            .keybindings
            .get(&Action::MoveLeft)
            .expect("MoveLeft action not found");
        assert!(move_left_bindings.contains(&"h".to_string()));
        assert!(move_left_bindings.contains(&"left".to_string()));
    }

    #[test]
    fn test_default_config_yaml() {
        let yaml = AppSettings::default_config_yaml();
        assert!(yaml.contains("database_url: sqlite://local.db"));
        assert!(yaml.contains("keybindings:"));
        assert!(yaml.contains("quit:"));

        // Verify it can be parsed back
        let settings: AppSettings =
            serde_yaml::from_str(&yaml).expect("Failed to parse generated YAML");
        assert_eq!(settings.database_url, Some("sqlite://local.db".to_string()));
        assert!(settings.keybindings.contains_key(&Action::Quit));
    }
}
