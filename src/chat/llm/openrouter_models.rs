//! OpenRouter model catalog helpers.

use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;

use crate::error::{MycryptoError, Result};

const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

#[derive(Debug, Clone, Deserialize)]
struct OpenRouterModelsResponse {
    data: Vec<OpenRouterModelEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenRouterModelEntry {
    id: String,
    pricing: Option<OpenRouterModelPricing>,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenRouterModelPricing {
    prompt: String,
    completion: String,
}

fn build_http_client() -> Result<Client> {
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(MycryptoError::Http)
}

fn extract_free_models(response: OpenRouterModelsResponse) -> Vec<String> {
    let mut models = response
        .data
        .into_iter()
        .filter_map(|entry| {
            let pricing = entry.pricing?;
            let id = entry.id;
            let id_is_free_variant = id.ends_with(":free") || id == "openrouter/free";
            if id_is_free_variant
                && pricing.prompt.trim() == "0"
                && pricing.completion.trim() == "0"
            {
                Some(id)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    models.sort();
    models.dedup();
    models
}

/// Fetches OpenRouter models and returns free model IDs.
#[must_use = "handle API and parsing errors when fetching model catalog"]
pub async fn fetch_openrouter_free_models(api_key: &str) -> Result<Vec<String>> {
    let client = build_http_client()?;
    let response = client
        .get(OPENROUTER_MODELS_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .header("HTTP-Referer", "https://github.com/mycrypto/mycrypto")
        .header("X-Title", "mycrypto")
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        let preview: String = body.chars().take(200).collect();
        return Err(MycryptoError::ApiError {
            api: "openrouter-models".to_string(),
            status,
            message: if body.chars().count() > 200 {
                format!("{}...", preview)
            } else {
                preview
            },
        });
    }

    let parsed = response.json::<OpenRouterModelsResponse>().await?;
    Ok(extract_free_models(parsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_free_models_requires_explicit_free_model_id() {
        let response = OpenRouterModelsResponse {
            data: vec![
                OpenRouterModelEntry {
                    id: "google/gemma-3n-e2b-it".to_string(),
                    pricing: Some(OpenRouterModelPricing {
                        prompt: "0".to_string(),
                        completion: "0".to_string(),
                    }),
                },
                OpenRouterModelEntry {
                    id: "google/gemma-3n-e2b-it:free".to_string(),
                    pricing: Some(OpenRouterModelPricing {
                        prompt: "0".to_string(),
                        completion: "0".to_string(),
                    }),
                },
                OpenRouterModelEntry {
                    id: "openrouter/free".to_string(),
                    pricing: Some(OpenRouterModelPricing {
                        prompt: "0".to_string(),
                        completion: "0".to_string(),
                    }),
                },
            ],
        };

        let result = extract_free_models(response);
        assert_eq!(
            result,
            vec![
                "google/gemma-3n-e2b-it:free".to_string(),
                "openrouter/free".to_string(),
            ]
        );
    }

    #[test]
    fn test_extract_free_models_filters_by_exact_zero_pricing() {
        let response = OpenRouterModelsResponse {
            data: vec![
                OpenRouterModelEntry {
                    id: "openrouter/free-1:free".to_string(),
                    pricing: Some(OpenRouterModelPricing {
                        prompt: "0".to_string(),
                        completion: "0".to_string(),
                    }),
                },
                OpenRouterModelEntry {
                    id: "openrouter/free-2:free".to_string(),
                    pricing: Some(OpenRouterModelPricing {
                        prompt: "0".to_string(),
                        completion: "0".to_string(),
                    }),
                },
                OpenRouterModelEntry {
                    id: "openrouter/paid".to_string(),
                    pricing: Some(OpenRouterModelPricing {
                        prompt: "0.000001".to_string(),
                        completion: "0.000002".to_string(),
                    }),
                },
                OpenRouterModelEntry {
                    id: "openrouter/no-pricing".to_string(),
                    pricing: None,
                },
                OpenRouterModelEntry {
                    id: "openrouter/free-1:free".to_string(),
                    pricing: Some(OpenRouterModelPricing {
                        prompt: "0".to_string(),
                        completion: "0".to_string(),
                    }),
                },
            ],
        };

        let result = extract_free_models(response);
        assert_eq!(
            result,
            vec![
                "openrouter/free-1:free".to_string(),
                "openrouter/free-2:free".to_string()
            ]
        );
    }
}
