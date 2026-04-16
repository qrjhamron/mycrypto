//! Configuration loading and environment variable resolution.
//!
//! This module handles:
//! - Loading configuration from TOML files
//! - Resolving `ENV:VAR_NAME` patterns to environment variable values
//! - Falling back to defaults when no config file exists
//!
//! # Security
//!
//! API keys and secrets should NEVER be stored in config files directly.
//! Instead, use the `ENV:` prefix to reference environment variables:
//! ```toml
//! [llm]
//! api_key = "ENV:CLAUDE_API_KEY"
//! ```

use std::env;
use std::path::{Path, PathBuf};

use crate::config::schema::Config;
use crate::error::{MycryptoError, Result};

/// The prefix used to indicate an environment variable reference.
const ENV_PREFIX: &str = "ENV:";

/// Default config file locations to search, in order of priority.
const DEFAULT_CONFIG_PATHS: &[&str] = &[
    "config.toml",
    "~/.config/mycrypto/config.toml",
    "~/.mycrypto/config.toml",
];

/// Loads configuration from the specified path or searches default locations.
///
/// # Arguments
/// * `path` - Optional explicit path to config file. If None, searches defaults.
///
/// # Returns
/// * `Ok(Config)` - Fully loaded and validated configuration
/// * `Err(MycryptoError)` - If loading, parsing, or validation fails
///
/// # Example
/// ```no_run
/// use mycrypto::config::load_config;
///
/// let config = load_config(Some("my-config.toml")).expect("failed to load config");
/// ```
pub fn load_config(path: Option<&str>) -> Result<Config> {
    let config_path = match path {
        Some(p) => Some(PathBuf::from(p)),
        None => find_config_file(),
    };

    let mut config = match config_path {
        Some(ref path) => load_from_file(path)?,
        None => {
            tracing::info!("no config file found, using defaults");
            Config::default()
        }
    };

    // Resolve environment variables in the config
    resolve_env_vars(&mut config)?;

    // Validate the final configuration
    config.validate()?;

    if let Some(ref path) = config_path {
        tracing::info!("loaded configuration from {}", path.display());
    }

    Ok(config)
}

/// Searches default locations for a config file.
///
/// Returns the first existing path found, or None if no config exists.
fn find_config_file() -> Option<PathBuf> {
    for path_str in DEFAULT_CONFIG_PATHS {
        let path = expand_tilde(path_str);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Expands `~` to the user's home directory.
fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs_home() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}

/// Gets the user's home directory.
fn dirs_home() -> Option<PathBuf> {
    env::var("HOME").ok().map(PathBuf::from)
}

/// Loads and parses configuration from a TOML file.
fn load_from_file(path: &Path) -> Result<Config> {
    let contents = std::fs::read_to_string(path).map_err(|source| MycryptoError::ConfigRead {
        path: path.display().to_string(),
        source,
    })?;

    let config: Config = toml::from_str(&contents)?;
    Ok(config)
}

/// Resolves all `ENV:VAR_NAME` patterns in the configuration.
///
/// Currently resolves:
/// - `llm.api_key`
/// - `llm.base_url` (if set)
///
/// Other fields can be added as needed.
fn resolve_env_vars(config: &mut Config) -> Result<()> {
    // Resolve LLM API key
    config.llm.api_key = resolve_env_value(&config.llm.api_key, "llm.api_key")?;

    // Resolve LLM base URL if set
    if let Some(ref url) = config.llm.base_url {
        let resolved = resolve_env_value(url, "llm.base_url")?;
        config.llm.base_url = Some(resolved);
    }

    Ok(())
}

/// Resolves a single value that may contain an `ENV:` prefix.
///
/// # Arguments
/// * `value` - The value to resolve (may or may not have ENV: prefix)
/// * `config_key` - The config key name (for error messages)
///
/// # Returns
/// * The resolved value (either the original or the env var value)
fn resolve_env_value(value: &str, config_key: &str) -> Result<String> {
    if is_env_reference(value) {
        let env_var_name = extract_env_var_name(value).unwrap_or_default();
        env::var(env_var_name).map_err(|_| MycryptoError::EnvVarNotFound {
            name: env_var_name.to_string(),
            config_key: config_key.to_string(),
        })
    } else {
        Ok(value.to_string())
    }
}

/// Creates a default config file at the specified path.
///
/// Useful for first-run setup or resetting configuration.
///
/// # Arguments
/// * `path` - Where to write the config file
///
/// # Returns
/// * `Ok(())` - If file was written successfully
/// * `Err(MycryptoError)` - If writing fails
pub fn create_default_config(path: &Path) -> Result<()> {
    let config = Config::default();
    let toml_str = toml::to_string_pretty(&config).map_err(|e| {
        MycryptoError::ConfigValidation(format!("failed to serialize config: {}", e))
    })?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(path, toml_str)?;
    tracing::info!("created default config at {}", path.display());
    Ok(())
}

/// Checks if a value is an environment variable reference.
pub fn is_env_reference(value: &str) -> bool {
    value.strip_prefix(ENV_PREFIX).is_some()
}

/// Extracts the environment variable name from an `ENV:` reference.
///
/// Returns `None` if the value is not an env reference.
pub fn extract_env_var_name(value: &str) -> Option<&str> {
    value.strip_prefix(ENV_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_resolve_env_value_no_prefix() {
        let result = resolve_env_value("plain-value", "test.key").unwrap();
        assert_eq!(result, "plain-value");
    }

    #[test]
    fn test_resolve_env_value_with_prefix() {
        env::set_var("TEST_MYCRYPTO_VAR", "secret123");
        let result = resolve_env_value("ENV:TEST_MYCRYPTO_VAR", "test.key").unwrap();
        assert_eq!(result, "secret123");
        env::remove_var("TEST_MYCRYPTO_VAR");
    }

    #[test]
    fn test_resolve_env_value_missing_var() {
        let result = resolve_env_value("ENV:NONEXISTENT_VAR_12345", "test.key");
        assert!(result.is_err());
        if let Err(MycryptoError::EnvVarNotFound { name, config_key }) = result {
            assert_eq!(name, "NONEXISTENT_VAR_12345");
            assert_eq!(config_key, "test.key");
        } else {
            panic!("expected EnvVarNotFound error");
        }
    }

    #[test]
    fn test_is_env_reference() {
        assert!(is_env_reference("ENV:MY_VAR"));
        assert!(!is_env_reference("plain-value"));
        assert!(!is_env_reference("env:lowercase"));
    }

    #[test]
    fn test_extract_env_var_name() {
        assert_eq!(extract_env_var_name("ENV:MY_VAR"), Some("MY_VAR"));
        assert_eq!(extract_env_var_name("plain"), None);
    }

    #[test]
    fn test_expand_tilde() {
        // Test with actual home
        if let Ok(home) = env::var("HOME") {
            let expanded = expand_tilde("~/test/path");
            assert!(expanded.starts_with(&home));
            assert!(expanded.to_string_lossy().ends_with("test/path"));
        }

        // Test without tilde
        let path = expand_tilde("/absolute/path");
        assert_eq!(path.to_string_lossy(), "/absolute/path");
    }

    #[test]
    fn test_default_config_loads() {
        // Should not panic when loading defaults
        env::set_var("CLAUDE_API_KEY", "test-key");
        let config = load_config(None);
        // May fail if env var not set, but shouldn't panic
        if config.is_ok() {
            assert!(config.unwrap().validate().is_ok());
        }
        env::remove_var("CLAUDE_API_KEY");
    }
}
