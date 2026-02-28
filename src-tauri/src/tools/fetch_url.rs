use futures::future::BoxFuture;
use reqwest::header::CONTENT_TYPE;
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use crate::tools::{ToolDef, ToolImpl};

const MAX_SUMMARY_CHARS: usize = 2000;
const REQUEST_TIMEOUT_SECONDS: u64 = 10;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

pub struct FetchUrlTool;

impl FetchUrlTool {
    pub fn new() -> Self {
        Self
    }
}

impl ToolImpl for FetchUrlTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            id: "fetch_url".to_string(),
            name: "fetch_url".to_string(),
            description: "Fetch URL contents and return readable text summary".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Absolute URL to fetch via HTTP GET"
                    }
                },
                "required": ["url"],
                "additionalProperties": false
            }),
        }
    }

    fn execute(&self, args: Value) -> BoxFuture<'_, Result<String, String>> {
        Box::pin(async move {
            let url = args
                .get("url")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if url.is_empty() {
                return Err("missing required argument: url".to_string());
            }

            let client = build_fetch_client()?;

            fetch_url_with_client(url, &client, false).await
        })
    }
}

fn build_fetch_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|err| err.to_string())
}

async fn fetch_url_with_client(
    raw_url: &str,
    client: &reqwest::Client,
    allow_private_hosts: bool,
) -> Result<String, String> {
    let parsed_url = reqwest::Url::parse(raw_url).map_err(|err| format!("invalid url: {err}"))?;
    validate_url_target(&parsed_url, allow_private_hosts)?;

    let response = client
        .get(parsed_url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "request failed with HTTP status {}",
            response.status()
        ));
    }

    let is_html = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase().contains("text/html"))
        .unwrap_or(false);

    let body = read_response_body_limited(response, MAX_RESPONSE_BYTES).await?;
    let readable = if is_html {
        extract_html_text(&body)?
    } else {
        sanitize_whitespace(&body)
    };
    let summary = truncate_chars(&readable, MAX_SUMMARY_CHARS);
    Ok(format!("Fetched URL content summary: {summary}"))
}

fn validate_url_target(url: &reqwest::Url, allow_private_hosts: bool) -> Result<(), String> {
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err("url scheme must be http or https".to_string()),
    }

    if allow_private_hosts {
        return Ok(());
    }

    let host = url
        .host_str()
        .ok_or_else(|| "url must include a valid host".to_string())?;
    if is_blocked_host(host) {
        return Err("url host is not allowed".to_string());
    }

    Ok(())
}

fn is_blocked_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    let host_for_ip_parse = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    let ip = match host_for_ip_parse.parse::<IpAddr>() {
        Ok(ip) => ip,
        Err(_) => return false,
    };

    match ip {
        IpAddr::V4(v4) => is_blocked_ipv4(v4),
        IpAddr::V6(v6) => is_blocked_ipv6(v6),
    }
}

fn is_blocked_ipv4(addr: Ipv4Addr) -> bool {
    if addr.is_loopback() || addr.is_unspecified() {
        return true;
    }

    let [a, b, ..] = addr.octets();
    matches!(
        (a, b),
        (10, _)
            | (172, 16..=31)
            | (192, 168)
            | (169, 254)
            | (100, 64..=127)
            | (198, 18..=19)
    )
}

fn is_blocked_ipv6(addr: Ipv6Addr) -> bool {
    if addr.is_loopback() || addr.is_unspecified() {
        return true;
    }

    let [a, b, ..] = addr.octets();
    matches!(
        (a, b),
        (0xfc..=0xfd, _) | (0xfe, 0x80..=0xbf) | (0xff, _)
    )
}

async fn read_response_body_limited(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> Result<String, String> {
    if let Some(content_length) = response.content_length() {
        if content_length > max_bytes as u64 {
            return Err(format!(
                "response body exceeds maximum allowed size of {max_bytes} bytes"
            ));
        }
    }

    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|err| err.to_string())? {
        if body.len() + chunk.len() > max_bytes {
            return Err(format!(
                "response body exceeds maximum allowed size of {max_bytes} bytes"
            ));
        }
        body.extend_from_slice(&chunk);
    }

    Ok(String::from_utf8_lossy(&body).into_owned())
}

fn extract_html_text(html: &str) -> Result<String, String> {
    let selector = Selector::parse("body").map_err(|err| err.to_string())?;
    let document = Html::parse_document(html);
    let text = document
        .select(&selector)
        .flat_map(|node| node.text())
        .collect::<Vec<_>>()
        .join(" ");
    if text.trim().is_empty() {
        return Ok(sanitize_whitespace(html));
    }
    Ok(sanitize_whitespace(&text))
}

fn sanitize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{build_fetch_client, fetch_url_with_client, FetchUrlTool, MAX_RESPONSE_BYTES};
    use crate::tools::ToolImpl;

    #[tokio::test]
    async fn fetch_url_extracts_readable_text_and_truncates_near_2000_chars() {
        let server = MockServer::start().await;
        let repeated = "Rust testing content for fetch_url tool. ".repeat(120);
        let html = format!(
            r#"<html><head><title>Example</title></head><body><main><h1>Header</h1><p>{repeated}</p></main></body></html>"#
        );

        Mock::given(method("GET"))
            .and(path("/page"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header("content-type", "text/html; charset=utf-8")
                    .set_body_string(html),
            )
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = fetch_url_with_client(&format!("{}/page", server.uri()), &client, true)
            .await
            .expect("fetch_url should return content");

        assert!(result.contains("Header"));
        assert!(result.contains("Rust testing content"));
        assert!(result.chars().count() <= 2100);
    }

    #[tokio::test]
    async fn fetch_url_rejects_non_http_scheme() {
        let tool = FetchUrlTool::new();
        let result = tool.execute(json!({ "url": "ftp://example.com/file" })).await;
        assert!(result.is_err());
        assert!(
            result
                .expect_err("invalid scheme should fail")
                .contains("http or https")
        );
    }

    #[tokio::test]
    async fn fetch_url_rejects_localhost_and_private_ipv4_literals() {
        let tool = FetchUrlTool::new();
        let blocked_urls = [
            "http://localhost:8080",
            "http://127.0.0.1/test",
            "http://0.0.0.0/api",
            "http://[::1]/",
            "http://10.1.2.3/secret",
            "http://172.16.1.2/data",
            "http://192.168.0.5/internal",
            "http://169.254.169.254/latest/meta-data",
            "http://100.64.10.20/internal",
            "http://198.18.0.1/bench",
            "http://[fc00::1]/",
            "http://[fe80::1]/",
            "http://[ff02::1]/",
        ];

        for url in blocked_urls {
            let result = tool.execute(json!({ "url": url })).await;
            let err = result.expect_err("expected blocked host error");
            assert!(
                err.contains("url host is not allowed"),
                "expected SSRF block error for {url}, got: {err}"
            );
        }
    }

    #[tokio::test]
    async fn fetch_url_returns_error_for_non_2xx_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/missing"))
            .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = fetch_url_with_client(&format!("{}/missing", server.uri()), &client, true).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn fetch_url_redirect_response_is_not_followed_and_returns_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/start"))
            .respond_with(ResponseTemplate::new(302).append_header("location", "/final"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/final"))
            .respond_with(ResponseTemplate::new(200).set_body_string("redirect target content"))
            .mount(&server)
            .await;

        let client = build_fetch_client().expect("client must build");
        let result = fetch_url_with_client(&format!("{}/start", server.uri()), &client, true).await;
        let err = result.expect_err("redirect should return 3xx status error");
        assert!(err.contains("302"), "expected 302 error, got: {err}");
    }

    #[tokio::test]
    async fn fetch_url_rejects_oversized_body() {
        let server = MockServer::start().await;
        let oversized = "x".repeat(MAX_RESPONSE_BYTES + 1);
        Mock::given(method("GET"))
            .and(path("/large"))
            .respond_with(ResponseTemplate::new(200).set_body_string(oversized))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let result = fetch_url_with_client(&format!("{}/large", server.uri()), &client, true).await;
        assert!(result.is_err());
        assert!(
            result
                .expect_err("oversized body should fail")
                .contains("maximum allowed size")
        );
    }
}
