use futures::future::BoxFuture;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

use crate::tools::{ToolDef, ToolImpl};

const MAX_RESULTS: usize = 5;
const DEFAULT_SNIPPET_CHARS: usize = 150;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

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
            let client = duckduckgo_search::DuckDuckGoSearch::new();
            let raw_results = client.search(query).await.map_err(|err| err.to_string())?;
            Ok(raw_results
                .into_iter()
                .map(|(text, url)| text_to_result(text, url))
                .collect())
        })
    }
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
            let results = tokio::time::timeout(self.timeout, self.provider.search(&query))
                .await
                .map_err(|_| "web search timed out".to_string())??;
            Ok(format_search_results(&results, DEFAULT_SNIPPET_CHARS))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::Arc;
    use std::time::Duration;

    use futures::future::BoxFuture;
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
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok(vec![])
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
}
