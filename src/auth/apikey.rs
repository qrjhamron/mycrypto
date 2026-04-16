//! API key storage for non-GitHub providers.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{MycryptoError as Error, Result};

use super::store::{mask_secret, AuthProvider, AuthStatus};

/// Stored API keys and related provider fields.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApiKeyStore {
    /// GitHub OAuth token.
    pub github_token: Option<String>,
    /// GitHub username for authenticated token.
    pub github_username: Option<String>,
    /// GitHub auth creation time.
    pub github_created_at: Option<DateTime<Utc>>,
    /// Anthropic key.
    pub anthropic: Option<String>,
    /// OpenAI key.
    pub openai: Option<String>,
    /// Gemini key.
    pub gemini: Option<String>,
    /// OpenRouter key.
    pub openrouter: Option<String>,
    /// Optional Gradio token.
    pub gradio_token: Option<String>,
    /// Gradio space URL.
    pub gradio_space_url: Option<String>,
}

fn home_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| Error::ConfigValidation("HOME environment variable not set".to_string()))?;
    Ok(PathBuf::from(home))
}

fn mycrypto_dir() -> Result<PathBuf> {
    let dir = home_dir()?.join(".mycrypto");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Returns the key store path.
pub fn keys_file_path() -> Result<PathBuf> {
    Ok(mycrypto_dir()?.join("keys.json"))
}

/// Load keys from disk.
pub fn load_keys() -> Result<ApiKeyStore> {
    let path = keys_file_path()?;
    if !path.exists() {
        return Ok(ApiKeyStore::default());
    }

    let raw = std::fs::read_to_string(path)?;
    let store: ApiKeyStore = serde_json::from_str(&raw)?;
    Ok(store)
}

/// Save keys to disk with restrictive file permissions.
pub fn save_keys(store: &ApiKeyStore) -> Result<()> {
    let path = keys_file_path()?;
    let serialized = serde_json::to_string_pretty(store)?;
    std::fs::write(&path, serialized)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = std::fs::metadata(&path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&path, perms)?;
    }

    Ok(())
}

/// Apply loaded keys to process environment for provider modules.
pub fn apply_keys_to_env(store: &ApiKeyStore) {
    set_or_remove_env("GITHUB_TOKEN", store.github_token.as_deref());
    set_or_remove_env("CLAUDE_API_KEY", store.anthropic.as_deref());
    set_or_remove_env("OPENAI_API_KEY", store.openai.as_deref());
    set_or_remove_env("GEMINI_API_KEY", store.gemini.as_deref());
    set_or_remove_env("OPENROUTER_API_KEY", store.openrouter.as_deref());
    set_or_remove_env("GRADIO_API_KEY", store.gradio_token.as_deref());
    set_or_remove_env("GRADIO_SPACE_URL", store.gradio_space_url.as_deref());
}

fn set_or_remove_env(name: &str, value: Option<&str>) {
    if let Some(v) = value {
        std::env::set_var(name, v);
    } else {
        std::env::remove_var(name);
    }
}

/// Set a provider key value.
pub fn set_api_key(provider: AuthProvider, value: String) -> Result<ApiKeyStore> {
    let mut store = load_keys()?;

    match provider {
        AuthProvider::Anthropic => store.anthropic = Some(value),
        AuthProvider::OpenAI => store.openai = Some(value),
        AuthProvider::Gemini => store.gemini = Some(value),
        AuthProvider::OpenRouter => store.openrouter = Some(value),
        AuthProvider::Gradio => store.gradio_token = Some(value),
        AuthProvider::GitHub => {
            return Err(Error::ConfigValidation(
                "GitHub token is managed by device flow".to_string(),
            ))
        }
    }

    save_keys(&store)?;
    Ok(store)
}

/// Set Gradio URL + optional token.
pub fn set_gradio(space_url: String, token: Option<String>) -> Result<ApiKeyStore> {
    let mut store = load_keys()?;
    store.gradio_space_url = Some(space_url);
    store.gradio_token = token;
    save_keys(&store)?;
    Ok(store)
}

/// Remove provider credentials.
pub fn remove_provider(provider: AuthProvider) -> Result<ApiKeyStore> {
    let mut store = load_keys()?;

    match provider {
        AuthProvider::GitHub => {
            store.github_token = None;
            store.github_username = None;
            store.github_created_at = None;
        }
        AuthProvider::Anthropic => store.anthropic = None,
        AuthProvider::OpenAI => store.openai = None,
        AuthProvider::Gemini => store.gemini = None,
        AuthProvider::OpenRouter => store.openrouter = None,
        AuthProvider::Gradio => {
            store.gradio_token = None;
            store.gradio_space_url = None;
        }
    }

    save_keys(&store)?;
    Ok(store)
}

/// Sync API key state into AppState auth map.
pub fn sync_auth_state(store: &ApiKeyStore, auth_state: &mut HashMap<AuthProvider, AuthStatus>) {
    if let (Some(token), Some(username), Some(created_at)) = (
        store.github_token.as_ref(),
        store.github_username.as_ref(),
        store.github_created_at,
    ) {
        auth_state.insert(
            AuthProvider::GitHub,
            AuthStatus::AuthenticatedGitHub {
                username: username.clone(),
                token: token.clone(),
                created_at,
            },
        );
    }

    if let Some(value) = &store.anthropic {
        auth_state.insert(
            AuthProvider::Anthropic,
            AuthStatus::ApiKeyConfigured {
                masked: mask_secret(value),
            },
        );
    }

    if let Some(value) = &store.openai {
        auth_state.insert(
            AuthProvider::OpenAI,
            AuthStatus::ApiKeyConfigured {
                masked: mask_secret(value),
            },
        );
    }

    if let Some(value) = &store.gemini {
        auth_state.insert(
            AuthProvider::Gemini,
            AuthStatus::ApiKeyConfigured {
                masked: mask_secret(value),
            },
        );
    }

    if let Some(value) = &store.openrouter {
        auth_state.insert(
            AuthProvider::OpenRouter,
            AuthStatus::ApiKeyConfigured {
                masked: mask_secret(value),
            },
        );
    }

    if store.gradio_space_url.is_some() || store.gradio_token.is_some() {
        auth_state.insert(
            AuthProvider::Gradio,
            AuthStatus::GradioConfigured {
                space_url: store
                    .gradio_space_url
                    .clone()
                    .unwrap_or_else(|| "(not set)".to_string()),
                token_masked: store.gradio_token.as_deref().map(mask_secret),
            },
        );
    }
}
