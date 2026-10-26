//! Bounded HTTPS transport for trusted plugins; never log request content.
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    url: String,
    method: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    body: Option<String>,
}
async fn send(request: &Request) -> Result<serde_json::Value, String> {
    let url = reqwest::Url::parse(&request.url).map_err(|_| "Invalid HTTPS URL")?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err("HTTP requires HTTPS without URL credentials".into());
    }
    if request.body.as_ref().is_some_and(|b| b.len() > 65_536)
        || request.url.len() > 8192
        || request
            .headers
            .iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>()
            > 65_536
    {
        return Err("HTTP request exceeds limit".into());
    }
    let method = reqwest::Method::from_bytes(request.method.as_bytes())
        .map_err(|_| "Invalid HTTP method")?;
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "HTTP transport unavailable")?;
    let mut builder = client.request(method, url);
    for (key, value) in &request.headers {
        builder = builder.header(key, value);
    }
    if let Some(body) = &request.body {
        builder = builder.body(body.clone());
    }
    let response = builder.send().await.map_err(|e| {
        if e.is_timeout() {
            "HTTP timed out"
        } else {
            "HTTPS request failed"
        }
    })?;
    read_response(response).await
}
async fn read_response(mut response: reqwest::Response) -> Result<serde_json::Value, String> {
    let status = response.status().as_u16();
    let headers: BTreeMap<_, _> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    if headers
        .iter()
        .map(|(key, value)| key.len() + value.len())
        .sum::<usize>()
        > 65_536
    {
        return Err("HTTP response headers exceed limit".into());
    }
    if response.content_length().is_some_and(|n| n > 524_288) {
        return Err("HTTP response exceeds 512 KiB".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "HTTP response failed")? {
        if bytes.len() + chunk.len() > 524_288 {
            return Err("HTTP response exceeds 512 KiB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(
        serde_json::json!({"status": status, "headers": headers, "body": String::from_utf8(bytes).map_err(|_| "HTTP response is not UTF-8")?}),
    )
}
pub async fn execute(
    request: &Request,
    job: Arc<crate::plugins::Job>,
) -> Result<serde_json::Value, String> {
    if job.cancel.load(Ordering::Acquire) {
        return Err("HTTP cancelled".into());
    }
    tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(20), send(request)) => result.map_err(|_| "HTTP timed out".to_string())?,
        _ = async { loop { if job.cancel.load(Ordering::Acquire) { break; } tokio::time::sleep(Duration::from_millis(10)).await; } } => Err("HTTP cancelled".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(url: &str, body: Option<String>) -> Request {
        Request {
            url: url.into(),
            method: "POST".into(),
            headers: BTreeMap::new(),
            body,
        }
    }
    #[test]
    fn rejects_insecure_urls_and_oversized_requests_before_network_io() {
        tauri::async_runtime::block_on(async {
            for url in [
                "http://example.com",
                "file:///tmp/x",
                "https://user:password@example.com",
            ] {
                assert!(send(&request(url, None))
                    .await
                    .unwrap_err()
                    .contains("HTTPS"));
            }
            assert!(
                send(&request("https://example.com", Some("x".repeat(65_537))))
                    .await
                    .unwrap_err()
                    .contains("exceeds")
            );
        });
    }
    #[test]
    fn response_limit_applies_with_and_without_content_length() {
        tauri::async_runtime::block_on(async {
            for length_header in [true, false] {
                for size in [524_288, 524_289] {
                    let mut response = http::Response::builder().status(200);
                    if length_header {
                        response = response.header("content-length", size.to_string());
                    }
                    let result =
                        read_response(response.body("x".repeat(size)).unwrap().into()).await;
                    if size == 524_288 {
                        assert_eq!(result.unwrap()["body"].as_str().unwrap().len(), size);
                    } else {
                        assert!(result.unwrap_err().contains("512 KiB"));
                    }
                }
            }
        });
    }
    #[test]
    fn cancellation_drops_pending_transport() {
        tauri::async_runtime::block_on(async {
            let job = Arc::new(crate::plugins::Job::default());
            job.cancel.store(true, Ordering::Release);
            let result = execute(&request("https://192.0.2.1", None), job).await;
            assert_eq!(result.unwrap_err(), "HTTP cancelled");
        });
    }
}
