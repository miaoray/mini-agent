use futures::future::BoxFuture;
use reqwest::Url;
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

use crate::tools::{ToolDef, ToolImpl};

const MAX_RESULTS: usize = 5;
const DEFAULT_SNIPPET_CHARS: usize = 150;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
const HTML_REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const FALLBACK_DDG_HTML_ENDPOINT: &str = "https://html.duckduckgo.com/html/";
const FALLBACK_USER_AGENT: &str =
    "mini-agent/0.1 (+https://github.com/miaoray/mini-agent)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub title: String,
    pub snippet: String,
    pub url: String,
}

pub fn format_search_results(results: &[SearchResult], max_per_result: usize) -> String {
    if results.is_empty() {
        return "No results found.".to_string();
    }

    let max_per_result = max_per_result.max(40);
    results
        .iter()
        .take(MAX_RESULTS)
        .enumerate()
        .map(|(idx, result)| {
            let clean_title = sanitize_whitespace(&result.title);
            let clean_snippet = sanitize_whitespace(&result.snippet);
            let clean_url = sanitize_whitespace(&result.url);

            let prefix = format!("{}. {} - ", idx + 1, clean_title);
            let suffix = format!(" ({clean_url})");
            let available = max_per_result.saturating_sub(prefix.chars().count() + suffix.chars().count());
            let snippet = truncate_with_ellipsis(&clean_snippet, available);

            format!("{prefix}{snippet}{suffix}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn sanitize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_with_ellipsis(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let chars: Vec<char> = input.chars().collect();
    if chars.len() <= max_chars {
        return input.to_string();
    }

    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }

    chars[..(max_chars - 3)].iter().collect::<String>() + "..."
}

fn text_to_result(text: String, url: String) -> SearchResult {
    if let Some((title, snippet)) = text.split_once(" - ") {
        SearchResult {
            title: title.to_string(),
            snippet: snippet.to_string(),
            url,
        }
    } else {
        SearchResult {
            title: text.clone(),
            snippet: text,
            url,
        }
    }
}

trait WebSearchProvider: Send + Sync {
    fn search<'a>(&'a self, query: &'a str) -> BoxFuture<'a, Result<Vec<SearchResult>, String>>;
}

struct DuckDuckGoProvider;

impl WebSearchProvider for DuckDuckGoProvider {
    fn search<'a>(&'a self, query: &'a str) -> BoxFuture<'a, Result<Vec<SearchResult>, String>> {
        Box::pin(async move {
            // Try HTML fallback first: we control its timeout (8s), and duckduckgo.com/html/
            // is typically faster than api.duckduckgo.com (SDK has no timeout, often slow/hangs).
            eprintln!(
                "[mini-agent][web_search] query={:?} provider=html_fallback (primary)",
                query
            );
            let html_results = search_duckduckgo_html_fallback(query).await;
            if let Ok(ref results) = html_results {
                if !results.is_empty() {
                    eprintln!(
                        "[mini-agent][web_search] html_fallback_results_count={} query={:?}",
                        results.len(),
                        query
                    );
                    return Ok(results.clone());
                }
            }
            if let Err(e) = &html_results {
                eprintln!(
                    "[mini-agent][web_search] html_fallback failed: {} query={:?}",
                    e, query
                );
            }

            // Fall back to SDK when HTML returns empty or errors.
            eprintln!(
                "[mini-agent][web_search] trying sdk fallback query={:?}",
                query
            );
            let client = duckduckgo_search::DuckDuckGoSearch::new();
            let raw_results = client.search(query).await.map_err(|err| err.to_string())?;
            let results: Vec<SearchResult> = raw_results
                .into_iter()
                .map(|(text, url)| text_to_result(text, url))
                .collect();
            eprintln!(
                "[mini-agent][web_search] sdk_results_count={} query={:?}",
                results.len(),
                query
            );
            Ok(results)
        })
    }
}

async fn search_duckduckgo_html_fallback(query: &str) -> Result<Vec<SearchResult>, String> {
    let mut url = Url::parse(FALLBACK_DDG_HTML_ENDPOINT).map_err(|err| err.to_string())?;
    url.query_pairs_mut().append_pair("q", query);
    let client = reqwest::Client::builder()
        .timeout(HTML_REQUEST_TIMEOUT)
        .build()
        .map_err(|err| err.to_string())?;
    let html = client
        .get(url)
        .header("user-agent", FALLBACK_USER_AGENT)
        .send()
        .await
        .map_err(|err| format!("fallback search request failed: {err}"))?
        .error_for_status()
        .map_err(|err| format!("fallback search request failed: {err}"))?
        .text()
        .await
        .map_err(|err| format!("fallback search body read failed: {err}"))?;
    Ok(parse_duckduckgo_html_results(&html))
}

fn parse_duckduckgo_html_results(html: &str) -> Vec<SearchResult> {
    let document = Html::parse_document(html);
    let title_selector = Selector::parse("a.result__a")
        .expect("hardcoded selector a.result__a should parse successfully");
    let snippet_selector = Selector::parse("a.result__snippet, div.result__snippet")
        .expect("hardcoded snippet selectors should parse successfully");

    let snippets: Vec<String> = document
        .select(&snippet_selector)
        .map(|node| sanitize_whitespace(&node.text().collect::<Vec<_>>().join(" ")))
        .collect();

    let mut results = Vec::new();
    for (idx, link) in document.select(&title_selector).enumerate() {
        let title = sanitize_whitespace(&link.text().collect::<Vec<_>>().join(" "));
        let url = link
            .value()
            .attr("href")
            .map(sanitize_whitespace)
            .unwrap_or_default();
        if title.is_empty() || url.is_empty() {
            continue;
        }
        let snippet = snippets.get(idx).cloned().unwrap_or_default();
        results.push(SearchResult {
            title,
            snippet,
            url,
        });
        if results.len() >= MAX_RESULTS {
            break;
        }
    }
    results
}

pub struct WebSearchTool {
    provider: Arc<dyn WebSearchProvider>,
    timeout: Duration,
}

impl WebSearchTool {
    pub fn new() -> Self {
        Self {
            provider: Arc::new(DuckDuckGoProvider),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    fn parse_query(args: &Value) -> Result<String, String> {
        let query = args
            .get("query")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string();

        if query.is_empty() {
            return Err("missing required argument: query".to_string());
        }

        Ok(query)
    }
}

impl ToolImpl for WebSearchTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            id: "web_search".to_string(),
            name: "web_search".to_string(),
            description: "Search the web for up-to-date information using DuckDuckGo".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query to run on DuckDuckGo"
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        }
    }

    fn execute(&self, args: Value) -> BoxFuture<'_, Result<String, String>> {
        Box::pin(async move {
            let query = Self::parse_query(&args)?;
            eprintln!("[mini-agent][web_search] execute query={:?}", query);
            let results = tokio::time::timeout(self.timeout, self.provider.search(&query))
                .await
                .map_err(|_| "web search timed out".to_string())??;
            eprintln!(
                "[mini-agent][web_search] execute completed query={:?} result_count={}",
                query,
                results.len()
            );
            Ok(format_search_results(&results, DEFAULT_SNIPPET_CHARS))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::Arc;
    use std::time::Duration;

    use futures::future::{pending, BoxFuture};
    use serde_json::json;

    use super::{format_search_results, SearchResult, WebSearchProvider, WebSearchTool};
    use crate::tools::ToolImpl;

    struct ErrorProvider;
    struct SlowProvider;

    impl WebSearchProvider for ErrorProvider {
        fn search<'a>(&'a self, _query: &'a str) -> BoxFuture<'a, Result<Vec<SearchResult>, String>> {
            Box::pin(async { Err("provider failed".to_string()) })
        }
    }

    impl WebSearchProvider for SlowProvider {
        fn search<'a>(&'a self, _query: &'a str) -> BoxFuture<'a, Result<Vec<SearchResult>, String>> {
            Box::pin(async {
                pending::<Result<Vec<SearchResult>, String>>().await
            })
        }
    }

    fn run_on_tokio<F>(future: F) -> Result<String, String>
    where
        F: Future<Output = Result<String, String>>,
    {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("tokio test runtime should build");
        runtime.block_on(future)
    }

    /// Runtime with IO for network tests (e.g. web_search_live).
    fn run_on_tokio_with_io<F>(future: F) -> Result<String, String>
    where
        F: Future<Output = Result<String, String>> + Send,
    {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .expect("tokio runtime with io should build");
        runtime.block_on(future)
    }

    #[test]
    fn format_search_results_handles_empty_results() {
        let formatted = format_search_results(&[], 150);
        assert_eq!(formatted, "No results found.");
    }

    #[test]
    fn format_search_results_formats_three_results_and_truncates() {
        let long_snippet = "Rust is a systems programming language focused on safety, speed, and concurrency. It empowers developers to build reliable and efficient software with fearless refactoring and strong tooling.".to_string();
        let results = vec![
            SearchResult {
                title: "Rust Programming Language".to_string(),
                snippet: long_snippet,
                url: "https://www.rust-lang.org".to_string(),
            },
            SearchResult {
                title: "The Rust Book".to_string(),
                snippet: "The official Rust book teaches ownership, borrowing, lifetimes, and practical examples.".to_string(),
                url: "https://doc.rust-lang.org/book/".to_string(),
            },
            SearchResult {
                title: "Rust by Example".to_string(),
                snippet: "A collection of runnable examples that demonstrate core Rust concepts.".to_string(),
                url: "https://doc.rust-lang.org/rust-by-example/".to_string(),
            },
        ];

        let formatted = format_search_results(&results, 150);

        let lines: Vec<&str> = formatted.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("1. Rust Programming Language - "));
        assert!(lines[0].contains("..."));
        assert!(lines[0].contains("https://www.rust-lang.org"));
        assert!(lines[1].starts_with("2. The Rust Book - "));
        assert!(lines[2].starts_with("3. Rust by Example - "));
    }

    #[test]
    fn format_search_results_caps_output_at_five_results() {
        let results: Vec<SearchResult> = (1..=8)
            .map(|i| SearchResult {
                title: format!("Title {i}"),
                snippet: format!("Snippet {i}"),
                url: format!("https://example.com/{i}"),
            })
            .collect();

        let formatted = format_search_results(&results, 150);
        let lines: Vec<&str> = formatted.lines().collect();

        assert_eq!(lines.len(), 5);
        assert!(lines[4].starts_with("5. Title 5 - "));
    }

    #[test]
    fn execute_returns_err_for_missing_query() {
        let tool = WebSearchTool::new();
        let result = run_on_tokio(tool.execute(json!({})));
        assert_eq!(result, Err("missing required argument: query".to_string()));
    }

    #[test]
    fn execute_returns_err_when_provider_fails() {
        let tool = WebSearchTool {
            provider: Arc::new(ErrorProvider),
            timeout: Duration::from_millis(50),
        };

        let result = run_on_tokio(tool.execute(json!({ "query": "rust" })));
        assert_eq!(result, Err("provider failed".to_string()));
    }

    #[test]
    fn execute_returns_err_when_search_times_out() {
        let tool = WebSearchTool {
            provider: Arc::new(SlowProvider),
            timeout: Duration::from_millis(50),
        };

        let result = run_on_tokio(tool.execute(json!({ "query": "rust" })));
        assert_eq!(result, Err("web search timed out".to_string()));
    }

    #[test]
    fn parse_duckduckgo_html_results_extracts_title_snippet_and_url() {
        let html = r#"
        <html>
          <body>
            <div class="result">
              <a class="result__a" href="https://example.com/a">Result A</a>
              <div class="result__snippet">Snippet A</div>
            </div>
            <div class="result">
              <a class="result__a" href="https://example.com/b">Result B</a>
              <a class="result__snippet">Snippet B</a>
            </div>
          </body>
        </html>
        "#;

        let parsed = super::parse_duckduckgo_html_results(html);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].title, "Result A");
        assert_eq!(parsed[0].snippet, "Snippet A");
        assert_eq!(parsed[0].url, "https://example.com/a");
        assert_eq!(parsed[1].title, "Result B");
        assert_eq!(parsed[1].snippet, "Snippet B");
        assert_eq!(parsed[1].url, "https://example.com/b");
    }

    /// Integration test: hits real DuckDuckGo. Run with `cargo test web_search_live -- --ignored`.
    #[test]
    #[ignore = "requires network; run with: cargo test web_search_live -- --ignored"]
    fn web_search_live_returns_results_within_timeout() {
        let tool = WebSearchTool::new();
        let result = run_on_tokio_with_io(tool.execute(json!({ "query": "rust programming" })));
        assert!(result.is_ok(), "web_search should succeed: {:?}", result);
        let output = result.unwrap();
        assert!(
            !output.is_empty() && output != "No results found.",
            "expected non-empty results, got: {:?}",
            output
        );
    }
}
