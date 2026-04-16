//! GitHub Device Authorization Flow (RFC 8628).

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{error, warn};

use crate::error::{MycryptoError as Error, Result};

use super::apikey::{load_keys, save_keys};

/// GitHub OAuth App client ID.
/// This is a public identifier, not a secret.
pub const GITHUB_CLIENT_ID: &str = "Iv23liYOUROAUTHAPPID";

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Debug, Deserialize)]
struct AccessTokenResponse {
    access_token: Option<String>,
    #[serde(rename = "token_type")]
    _token_type: Option<String>,
    #[serde(rename = "scope")]
    _scope: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubUser {
    login: String,
    #[serde(rename = "name")]
    _name: Option<String>,
}

/// Stored GitHub authentication data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredGithubAuth {
    /// OAuth access token.
    pub access_token: String,
    /// GitHub username.
    pub username: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Events produced by device flow.
#[derive(Debug, Clone)]
pub enum AuthEvent {
    /// Device code obtained and ready for user authorization.
    DeviceCode {
        /// User code to type on verification page.
        user_code: String,
        /// Verification URI.
        verification_uri: String,
        /// Expiration duration from now.
        expires_in: Duration,
        /// Poll interval in seconds.
        interval_secs: u64,
    },
    /// Polling status update.
    Polling {
        /// Remaining validity duration.
        remaining: Duration,
    },
    /// Flow succeeded.
    Success {
        /// GitHub username.
        username: String,
        /// OAuth token.
        token: String,
    },
    /// Flow expired.
    Expired,
    /// Recoverable or terminal error in auth flow.
    Error(String),
}

/// GitHub authentication handler.
#[derive(Clone)]
pub struct GitHubAuth {
    client: reqwest::Client,
    client_id: String,
}

impl Default for GitHubAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl GitHubAuth {
    /// Create a new GitHub auth handler.
    pub fn new() -> Self {
        let env_client_id = std::env::var("GITHUB_CLIENT_ID")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| GITHUB_CLIENT_ID.to_string());
        Self {
            client: reqwest::Client::new(),
            client_id: env_client_id,
        }
    }

    /// Create with custom client ID.
    pub fn with_client_id(client_id: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            client_id: client_id.to_string(),
        }
    }

    /// Load stored auth from disk.
    pub fn load_stored() -> Result<Option<StoredGithubAuth>> {
        let store = load_keys()?;
        match (
            store.github_token,
            store.github_username,
            store.github_created_at,
        ) {
            (Some(access_token), Some(username), Some(created_at)) => Ok(Some(StoredGithubAuth {
                access_token,
                username,
                created_at,
            })),
            _ => Ok(None),
        }
    }

    fn save_auth(stored: &StoredGithubAuth) -> Result<()> {
        let mut store = load_keys()?;
        store.github_token = Some(stored.access_token.clone());
        store.github_username = Some(stored.username.clone());
        store.github_created_at = Some(stored.created_at);
        save_keys(&store)
    }

    /// Remove stored auth.
    pub fn logout() -> Result<()> {
        let mut store = load_keys()?;
        store.github_token = None;
        store.github_username = None;
        store.github_created_at = None;
        save_keys(&store)
    }

    /// Start GitHub Device Flow.
    ///
    /// Returns an event receiver; errors are emitted as `AuthEvent::Error`.
    pub async fn start_device_flow(&self) -> Result<mpsc::Receiver<AuthEvent>> {
        if self.client_id.trim().is_empty() || self.client_id == GITHUB_CLIENT_ID {
            return Err(Error::ConfigValidation(
                "Set GITHUB_CLIENT_ID in .env first".to_string(),
            ));
        }

        let (tx, rx) = mpsc::channel(32);

        let device_code = match self.request_device_code().await {
            Ok(response) => response,
            Err(err) => {
                let _ = tx.send(AuthEvent::Error(err.to_string())).await;
                return Ok(rx);
            }
        };

        let _ = tx
            .send(AuthEvent::DeviceCode {
                user_code: device_code.user_code.clone(),
                verification_uri: device_code.verification_uri.clone(),
                expires_in: Duration::from_secs(device_code.expires_in),
                interval_secs: device_code.interval.max(5),
            })
            .await;

        let client = self.client.clone();
        let client_id = self.client_id.clone();

        tokio::spawn(async move {
            let deadline = Instant::now() + Duration::from_secs(device_code.expires_in);
            let mut poll_interval = Duration::from_secs(device_code.interval.max(5));

            loop {
                let now = Instant::now();
                if now >= deadline {
                    let _ = tx.send(AuthEvent::Expired).await;
                    break;
                }

                let _ = tx
                    .send(AuthEvent::Polling {
                        remaining: deadline.saturating_duration_since(now),
                    })
                    .await;

                tokio::time::sleep(poll_interval).await;

                match Self::request_access_token(&client, &client_id, &device_code.device_code)
                    .await
                {
                    Ok(token_response) => {
                        if let Some(token) = token_response.access_token {
                            match Self::fetch_user(&client, &token).await {
                                Ok(username) => {
                                    let stored = StoredGithubAuth {
                                        access_token: token.clone(),
                                        username: username.clone(),
                                        created_at: Utc::now(),
                                    };

                                    if let Err(err) = Self::save_auth(&stored) {
                                        warn!("Failed to persist GitHub auth: {}", err);
                                    }

                                    let _ = tx.send(AuthEvent::Success { username, token }).await;
                                    break;
                                }
                                Err(err) => {
                                    let _ = tx.send(AuthEvent::Error(err.to_string())).await;
                                    break;
                                }
                            }
                        }

                        if let Some(code) = token_response.error {
                            match code.as_str() {
                                "authorization_pending" => continue,
                                "slow_down" => {
                                    poll_interval += Duration::from_secs(5);
                                    continue;
                                }
                                "expired_token" => {
                                    let _ = tx.send(AuthEvent::Expired).await;
                                    break;
                                }
                                "access_denied" => {
                                    let _ = tx
                                        .send(AuthEvent::Error("Access denied by user".to_string()))
                                        .await;
                                    break;
                                }
                                _ => {
                                    let detail =
                                        token_response.error_description.unwrap_or_default();
                                    let _ = tx
                                        .send(AuthEvent::Error(format!(
                                            "GitHub token exchange failed: {} {}",
                                            code, detail
                                        )))
                                        .await;
                                    break;
                                }
                            }
                        } else {
                            let _ = tx
                                .send(AuthEvent::Error(
                                    "GitHub token exchange returned no token".to_string(),
                                ))
                                .await;
                            break;
                        }
                    }
                    Err(err) => {
                        error!("GitHub token poll error: {}", err);
                        let _ = tx
                            .send(AuthEvent::Error(format!(
                                "Network error while polling token endpoint: {} [R]etry",
                                err
                            )))
                            .await;
                        // Continue polling until deadline.
                        continue;
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn request_device_code(&self) -> Result<DeviceCodeResponse> {
        let response = self
            .client
            .post("https://github.com/login/device/code")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("scope", "read:user"),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::ApiError {
                api: "github/device/code".to_string(),
                status,
                message: body,
            });
        }

        let payload = response.json::<DeviceCodeResponse>().await?;
        Ok(payload)
    }

    async fn request_access_token(
        client: &reqwest::Client,
        client_id: &str,
        device_code: &str,
    ) -> Result<AccessTokenResponse> {
        let response = client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", client_id),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::ApiError {
                api: "github/oauth/access_token".to_string(),
                status,
                message: body,
            });
        }

        let payload = response.json::<AccessTokenResponse>().await?;
        Ok(payload)
    }

    async fn fetch_user(client: &reqwest::Client, token: &str) -> Result<String> {
        let response = client
            .get("https://api.github.com/user")
            .header("Authorization", format!("Bearer {}", token))
            .header("User-Agent", "mYcrypto")
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(Error::ApiError {
                api: "github/user".to_string(),
                status,
                message: body,
            });
        }

        let user = response.json::<GitHubUser>().await?;
        Ok(user.login)
    }

    /// Verify an existing token and return username.
    pub async fn verify_token(token: &str) -> Result<String> {
        let client = reqwest::Client::new();
        Self::fetch_user(&client, token).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_auth_creation() {
        let auth = GitHubAuth::with_client_id("test-client");
        assert_eq!(auth.client_id, "test-client".to_string());
    }
}
