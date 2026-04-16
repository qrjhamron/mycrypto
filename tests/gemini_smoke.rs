use std::env;
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use serde_json::{json, Value};

fn load_api_key() -> Option<String> {
    ["GEMINI_API_KEY", "GOOGLE_API_KEY"]
        .iter()
        .find_map(|name| env::var(name).ok().map(|v| v.trim().to_string()))
        .filter(|v| !v.is_empty())
}

fn preview_first_100_chars(input: &str) -> String {
    let preview: String = input.chars().take(100).collect();
    if input.chars().count() > 100 {
        format!("{}...", preview)
    } else {
        preview
    }
}

#[tokio::test]
async fn gemini_generate_content_smoke() {
    let key = match load_api_key() {
        Some(v) => v,
        None => {
            println!(
                "SKIP: GEMINI_API_KEY or GOOGLE_API_KEY is not set (or empty), skipping smoke test"
            );
            return;
        }
    };

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent?key={}",
        key
    );

    let payload = json!({
        "contents": [
            {
                "parts": [
                    {
                        "text": "say hello"
                    }
                ]
            }
        ]
    });

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            println!(
                "gemini_smoke error: status=N/A latency_ms=0 response_first_100=N/A error={} ",
                err
            );
            panic!("failed to build reqwest client: {}", err);
        }
    };

    let start = Instant::now();
    let response_result = client.post(&url).json(&payload).send().await;
    let latency_ms = start.elapsed().as_millis();

    let response = match response_result {
        Ok(resp) => resp,
        Err(err) => {
            println!(
                "gemini_smoke error: status=N/A latency_ms={} response_first_100=N/A error={}",
                latency_ms, err
            );
            panic!("Gemini request failed: {}", err);
        }
    };

    let status = response.status();
    let body = match response.text().await {
        Ok(body) => body,
        Err(err) => {
            println!(
                "gemini_smoke error: status={} latency_ms={} response_first_100=N/A error={}",
                status.as_u16(),
                latency_ms,
                err
            );
            panic!("failed reading response body: {}", err);
        }
    };
    let preview = preview_first_100_chars(&body);

    println!(
        "gemini_smoke result: status={} latency_ms={} response_first_100={} error=none",
        status.as_u16(),
        latency_ms,
        preview
    );

    if status != StatusCode::OK {
        println!("gemini_smoke full_response_body_on_failure: {}", body);
    }
    assert_eq!(status, StatusCode::OK, "expected HTTP 200 from Gemini API");

    let parsed: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(err) => {
            println!("gemini_smoke full_response_body_on_failure: {}", body);
            panic!("failed parsing JSON response: {}", err);
        }
    };

    let text = parsed
        .pointer("/candidates/0/content/parts/0/text")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if text.trim().is_empty() {
        println!("gemini_smoke full_response_body_on_failure: {}", body);
    }

    assert!(
        !text.trim().is_empty(),
        "expected non-empty candidates[0].content.parts[0].text"
    );
}
