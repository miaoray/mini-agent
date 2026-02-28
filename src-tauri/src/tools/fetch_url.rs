use futures::future::BoxFuture;
use reqwest::header::CONTENT_TYPE;
use scraper::{Html, Selector};
use serde_json::{json, Value};

use crate::tools::{ToolDef, ToolImpl};

const MAX_SUMMARY_CHARS: usize = 2000;

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

            let response = reqwest::get(url).await.map_err(|err| err.to_string())?;
            let is_html = response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(|value| value.to_ascii_lowercase().contains("text/html"))
                .unwrap_or(false);
            let body = response.text().await.map_err(|err| err.to_string())?;

            let readable = if is_html {
                extract_html_text(&body)?
            } else {
                sanitize_whitespace(&body)
            };
            let summary = truncate_chars(&readable, MAX_SUMMARY_CHARS);
            Ok(format!("Fetched URL content summary: {summary}"))
        })
    }
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

    use super::FetchUrlTool;
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

        let tool = FetchUrlTool::new();
        let result = tool
            .execute(json!({ "url": format!("{}/page", server.uri()) }))
            .await
            .expect("fetch_url should return content");

        assert!(result.contains("Header"));
        assert!(result.contains("Rust testing content"));
        assert!(result.chars().count() <= 2100);
    }
}
