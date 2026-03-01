use futures::future::BoxFuture;
use reqwest::header::{CONTENT_TYPE, LOCATION};
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use crate::tools::{ToolDef, ToolImpl};

const MAX_SUMMARY_CHARS: usize = 2000;
const REQUEST_TIMEOUT_SECONDS: u64 = 10;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

pub struct FetchUrlTool;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidatedDnsTarget {
    host: String,
    addrs: Vec<SocketAddr>,
}

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
    build_bound_fetch_client(None)
}

async fn fetch_url_with_client(
    raw_url: &str,
    client: &reqwest::Client,
    allow_private_hosts: bool,
) -> Result<String, String> {
    let mut current_url = reqwest::Url::parse(raw_url).map_err(|err| format!("invalid url: {err}"))?;
    let mut redirect_count = 0usize;

    loop {
        let validated_target = validate_url_target(&current_url, allow_private_hosts).await?;
        let response = if allow_private_hosts {
            client
                .get(current_url.clone())
                .send()
                .await
                .map_err(|err| err.to_string())?
        } else if let Some(target) = validated_target.as_ref() {
            send_with_bound_target(&current_url, target).await?
        } else {
            client
                .get(current_url.clone())
                .send()
                .await
                .map_err(|err| err.to_string())?
        };

        if response.status().is_redirection() {
            if redirect_count >= MAX_REDIRECTS {
                return Err(format!("too many redirects (>{MAX_REDIRECTS})"));
            }
            let location = response
                .headers()
                .get(LOCATION)
                .ok_or_else(|| "redirect response missing Location header".to_string())?
                .to_str()
                .map_err(|_| "redirect Location header is not valid UTF-8".to_string())?;
            current_url = current_url
                .join(location)
                .map_err(|err| format!("invalid redirect location: {err}"))?;
            redirect_count += 1;
            continue;
        }

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
        return Ok(format!("Fetched URL content summary: {summary}"));
    }
}

async fn send_with_bound_target(
    parsed_url: &reqwest::Url,
    target: &ValidatedDnsTarget,
) -> Result<reqwest::Response, String> {
    let mut errors = Vec::new();
    for addr in &target.addrs {
        let single_addr_target = ValidatedDnsTarget {
            host: target.host.clone(),
            addrs: vec![*addr],
        };
        let client = build_bound_fetch_client(Some(&single_addr_target))?;
        match client.get(parsed_url.clone()).send().await {
            Ok(response) => return Ok(response),
            Err(err) => {
                errors.push(format!("{addr}: {err}"));
            }
        }
    }

    Err(format!(
        "all resolved addresses failed for host {}: {}",
        target.host,
        errors.join(" | ")
    ))
}

async fn validate_url_target(
    url: &reqwest::Url,
    allow_private_hosts: bool,
) -> Result<Option<ValidatedDnsTarget>, String> {
    validate_url_target_with_resolver(url, allow_private_hosts, |host, port| {
        Box::pin(resolve_host_socket_addrs(host.to_string(), port))
    })
    .await
}

async fn validate_url_target_with_resolver<F>(
    url: &reqwest::Url,
    allow_private_hosts: bool,
    resolver: F,
) -> Result<Option<ValidatedDnsTarget>, String>
where
    F: Fn(&str, u16) -> BoxFuture<'static, Result<Vec<SocketAddr>, String>>,
{
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err("url scheme must be http or https".to_string()),
    }

    if allow_private_hosts {
        return Ok(None);
    }

    let host = url
        .host_str()
        .ok_or_else(|| "url must include a valid host".to_string())?;
    if is_blocked_host(host) {
        return Err("url host is not allowed".to_string());
    }
    if !is_ip_literal(host) {
        let port = url
            .port_or_known_default()
            .ok_or_else(|| "url must include a valid host".to_string())?;
        let resolved_addrs = resolver(host, port).await?;
        if resolved_addrs.is_empty() {
            return Err("failed to resolve host: no addresses found".to_string());
        }
        if resolved_addrs.iter().any(|addr| is_blocked_ip(addr.ip())) {
            return Err("url host is not allowed".to_string());
        }
        return Ok(Some(ValidatedDnsTarget {
            host: host.to_string(),
            addrs: resolved_addrs,
        }));
    }

    Ok(None)
}

async fn resolve_host_socket_addrs(host: String, port: u16) -> Result<Vec<SocketAddr>, String> {
    let resolved = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|err| format!("failed to resolve host: {err}"))?;
    Ok(resolved.collect())
}

fn build_bound_fetch_client(target: Option<&ValidatedDnsTarget>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
        .redirect(reqwest::redirect::Policy::none());

    if let Some(target) = target {
        if target.addrs.is_empty() {
            return Err("failed to resolve host: no addresses found".to_string());
        }
        for addr in &target.addrs {
            builder = builder.resolve(&target.host, *addr);
        }
    }

    builder.build().map_err(|err| err.to_string())
}

fn is_ip_literal(host: &str) -> bool {
    let host_for_ip_parse = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    host_for_ip_parse.parse::<IpAddr>().is_ok()
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

fn is_blocked_ip(ip: IpAddr) -> bool {
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
    if let Some(mapped) = addr.to_ipv4_mapped() {
        return is_blocked_ipv4(mapped);
    }

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
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{
        build_bound_fetch_client, build_fetch_client, fetch_url_with_client, send_with_bound_target,
        validate_url_target_with_resolver, FetchUrlTool, ValidatedDnsTarget, MAX_RESPONSE_BYTES,
    };
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
            "http://[::ffff:127.0.0.1]/",
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
    async fn fetch_url_rejects_hostname_when_dns_resolves_to_private_ip() {
        let parsed = reqwest::Url::parse("https://blocked.test/resource")
            .expect("test URL should parse successfully");
        let result = validate_url_target_with_resolver(&parsed, false, |_host, _port| {
            Box::pin(async {
                Ok(vec![SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)),
                    443,
                )])
            })
        })
        .await;
        let err = result.expect_err("expected SSRF block for private DNS result");
        assert!(
            err.contains("url host is not allowed"),
            "expected SSRF block error, got: {err}"
        );
    }

    #[tokio::test]
    async fn validate_url_target_returns_resolved_public_socket_addrs_for_hostname() {
        let parsed = reqwest::Url::parse("https://allowed.test/resource")
            .expect("test URL should parse successfully");
        let result = validate_url_target_with_resolver(&parsed, false, |_host, _port| {
            Box::pin(async {
                Ok(vec![SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)),
                    443,
                )])
            })
        })
        .await
        .expect("expected successful validation");

        assert_eq!(
            result,
            Some(ValidatedDnsTarget {
                host: "allowed.test".to_string(),
                addrs: vec![SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)),
                    443
                )],
            })
        );
    }

    #[test]
    fn build_bound_fetch_client_rejects_empty_resolved_address_list() {
        let err = build_bound_fetch_client(Some(&ValidatedDnsTarget {
            host: "example.com".to_string(),
            addrs: Vec::new(),
        }))
        .expect_err("expected empty bound addrs to be rejected");
        assert!(err.contains("failed to resolve host"));
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
    async fn fetch_url_follows_safe_redirect_and_returns_content() {
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
        let result = fetch_url_with_client(&format!("{}/start", server.uri()), &client, true)
            .await
            .expect("redirect should be followed manually");
        assert!(
            result.contains("redirect target content"),
            "expected final response body in summary, got: {result}"
        );
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

    #[tokio::test]
    async fn send_with_bound_target_retries_when_first_address_fails() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/retry"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;

        let parsed = reqwest::Url::parse("http://retry.test/retry")
            .expect("retry test URL should parse successfully");
        let target = ValidatedDnsTarget {
            host: "retry.test".to_string(),
            // First address should fail quickly (connection refused), second should succeed.
            addrs: vec![
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9),
                SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), server.address().port()),
            ],
        };

        let response = send_with_bound_target(&parsed, &target)
            .await
            .expect("request should succeed with fallback address");
        assert_eq!(response.status().as_u16(), 200);
    }
}
