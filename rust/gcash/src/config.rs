use config::{Config, File, FileFormat};
use serde::Deserialize;
use crate::error::AppError;

/// The central application configuration.
#[derive(Debug, Deserialize)]
pub struct AppSettings {
    pub database_url: Option<String>,
}

/// Loads configuration settings using a hierarchical fallback mechanism:
/// 1. `--config PATH` (CLI Argument - Highest priority)
/// 2. `~/.config/gcash/default.yml`
/// 3. `/etc/gcash/default.yml` (Lowest priority)
pub fn load_config(cli_config_path: Option<&String>) -> Result<AppSettings, AppError> {
    let mut builder = Config::builder();

    // 3. Lowest Priority: System-wide default
    builder = builder.add_source(File::new("/etc/gcash/default.yml", FileFormat::Yaml).required(false));

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
        let config_content = unindent(r#"
            database_url: "sqlite://test.db"
        "#);

        // Tempfile ensures isolated test environments that clean themselves up
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        write!(temp_file, "{}", config_content).expect("Failed to write to temp file");

        let temp_path = temp_file.path().to_string_lossy().to_string();
        let settings = load_config(Some(&temp_path)).expect("Failed to load config");

        assert_eq!(settings.database_url, Some("sqlite://test.db".to_string()));
    }
}
