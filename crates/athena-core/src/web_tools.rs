
//! Opt-in outbound tools for the chat orchestrator. Both calls go through a
//! blocking reqwest client (the executor runs under `spawn_blocking`); the
//! direct HTTPS request stays within the app process and sandbox policy
//! (no command-line curl, no shell).

use std::io::Read;

const MAX_BODY_BYTES: usize = 256 * 1024;

#[cfg(test)]
fn blocking_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::limited(3))
        .user_agent(concat!("athena-core/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())
}

fn trim_html(body: &str) -> String {
    let re = regex::Regex::new(r"<script[\s\S]*?</script>|<style[\s\S]*?</style>").unwrap();
    let b = re.replace_all(body, "");
    let re = regex::Regex::new(r"<[^>]+>").unwrap();
    let b = re.replace_all(&b, " ");
    // Collapse whitespace; html entities we only normalize the common ones.
    let b = b
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    let mut out = String::with_capacity(b.len());
    let mut last_ws = true;
    for ch in b.chars() {
        if ch.is_whitespace() {
            if !last_ws { out.push(' '); }
            last_ws = true;
        } else {
            out.push(ch);
            last_ws = false;
        }
    }
    out.trim().chars().take(MAX_BODY_BYTES).collect()
}

/// GET a URL with a prebuilt client (the executor builds it inside
/// `block_in_place` so blocking is legal on the tokio runtime).
pub fn web_fetch_with_client(client: &reqwest::blocking::Client, url: &str) -> Result<String, String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("url must start with http:// or https://".into());
    }
    let mut resp = client.get(url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let mut buf = Vec::new();
    resp.by_ref().take(MAX_BODY_BYTES as u64).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    let body = String::from_utf8_lossy(&buf);
    if resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| t.contains("html"))
    {
        Ok(trim_html(&body))
    } else {
        Ok(body.to_string())
    }
}

fn ddg_result_lines(html: &str) -> Vec<String> {
    // DuckDuckGo HTML endpoint markup: title links in class="result__a",
    // snippets in class="result__snippet". Regex over the trimmed DOM is
    // fragile but sufficient for a text summary feed.
    let title_re = regex::Regex::new(r#"<a[^>]*class="result__a"[^>]*>([\s\S]*?)</a>"#).unwrap();
    let snippet_re = regex::Regex::new(r#"<a[^>]*class="result__snippet"[^>]*>([\s\S]*?)</a>|(?:<div[^>]*class="result__snippet"[^>]*>([\s\S]*?)</div>)"#).unwrap();
    let href_re = regex::Regex::new(r#"href="([^"]+)""#).unwrap();
    let strip = |s: &str| {
        let re2 = regex::Regex::new(r"<[^>]+>").unwrap();
        re2.replace_all(s, "").to_string()
    };
    let titles: Vec<String> = title_re.captures_iter(html).map(|c| strip(&c[1])).collect();
    let snippets: Vec<String> = snippet_re
        .captures_iter(html)
        .map(|c| strip(c.get(1).or(c.get(2)).map(|m| m.as_str()).unwrap_or("")))
        .collect();
    let hrefs: Vec<String> = title_re
        .captures_iter(html)
        .filter_map(|c| {
            c.get(0).and_then(|m| href_re.captures(m.as_str()).map(|h| h[1].to_string()))
        })
        .collect();
    titles
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            let url = hrefs.get(i).cloned().unwrap_or_default();
            let snip = snippets.get(i).cloned().unwrap_or_default();
            if snip.is_empty() {
                format!("{i}. {t} — {url}", i = i + 1, t = t, url = url)
            } else {
                format!("{i}. {t} — {url}\n   {snip}", i = i + 1, t = t, url = url, snip = snip)
            }
        })
        .collect()
}

/// Web search via DuckDuckGo's HTML endpoint (no API key required). Other
/// providers can be layered on later; today anything other than "duckduckgo"
/// falls through the same path with a note.
pub fn web_search_with_client(
    client: &reqwest::blocking::Client,
    query: &str,
    provider: Option<&str>,
) -> Result<String, String> {
    let provider = provider.unwrap_or("duckduckgo");
    // Unknown providers still work but go to DDG (the no-key fallback).
    let _ = provider;
    let url = format!(
        "https://html.duckduckgo.com/html/?q={}",
        urlcode_string(query)
    );
    let mut resp = client.get(&url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let mut buf = Vec::new();
    resp.by_ref().take(MAX_BODY_BYTES as u64).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    let html = String::from_utf8_lossy(&buf);
    let lines = ddg_result_lines(&html);
    if lines.is_empty() {
        Ok(trim_html(&html).chars().take(4_000).collect())
    } else {
        Ok(lines.join("\n"))
    }
}

/// Minimal percent-encoding for the query slot.
fn urlcode_string(q: &str) -> String {
    q.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
                c.to_string()
            } else {
                format!("%{:02X}", c as u32)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ddg_parse_fixture() {
        let html = r#"
        <html><body>
          <div class="result" data-index="0" data-loc="xx\">
            <div class="result__body">
              <h2 class="result__title"><a class="result__a" href="https://rust-lang.org/">Rust – official site</a></h2>
              <a class="result__snippet" href="https://rust-lang.org/">Rust is a systems programming language.</a>
            </div>
          </div>
        </body></html>"#;
        let lines = ddg_result_lines(html);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("Rust"), "{lines:?}");
        assert!(lines[0].contains("rust-lang.org"), "{lines:?}");
    }

    #[test]
    fn trim_html_collapses_tags_and_space() {
        assert_eq!(trim_html("<b>Hello</b> <i>world</i>"), "Hello world");
    }

    #[test]
    fn web_fetch_rejects_non_http() {
        // Only the URL scheme check runs before any network I/O.
        let client = blocking_client().unwrap();
        assert!(web_fetch_with_client(&client, "file:///etc/passwd").is_err());
        assert!(web_fetch_with_client(&client, "ftp://x").is_err());
    }

    #[test]
    fn urlcode_escapes_spaces_and_symbols() {
        assert_eq!(urlcode_string("a b+c"), "a%20b%2Bc");
    }
}
