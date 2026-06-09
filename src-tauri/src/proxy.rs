use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Shared browser proxy state — includes the bound port so the frontend can build
/// iframe src URLs pointing at it.
pub struct BrowserProxy {
    pub port: u16,
}

impl BrowserProxy {
    /// Start the proxy on a random available port.
    pub fn start() -> Result<Self, String> {
        let port = Self::find_available_port()?;
        Self::spawn_server(port);
        Ok(Self { port })
    }

    /// Build a proxy URL for a given target.
    pub fn url_for(&self, target: &str) -> String {
        format!(
            "http://localhost:{}/proxy?url={}",
            self.port,
            url_escape(target)
        )
    }

    /// Find an available port on localhost.
    fn find_available_port() -> Result<u16, String> {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let addr = listener.local_addr().map_err(|e| e.to_string())?;
        drop(listener);
        Ok(addr.port())
    }

    /// Spawn the proxy server in a background thread.
    fn spawn_server(port: u16) {
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    log::error!("Browser proxy: failed to create tokio runtime: {}", e);
                    return;
                }
            };
            rt.block_on(async {
                if let Err(e) = run_server(port).await {
                    log::error!("Browser proxy server error: {}", e);
                }
            });
        });
    }
}

/// URL-encode a string manually (percent-encoding for unsafe chars).
fn url_escape(input: &str) -> String {
    let mut result = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                result.push(byte as char);
            }
            b' ' => result.push('+'),
            _ => {
                result.push('%');
                result.push_str(&format!("{:02X}", byte));
            }
        }
    }
    result
}

/// Run the HTTP proxy server.
async fn run_server(port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    log::info!("Browser proxy listening on http://{}", addr);

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15")
        .build()?;
    let req_counter = Arc::new(AtomicU16::new(0));

    loop {
        let (stream, _) = listener.accept().await?;
        let client = client.clone();
        let counter = Arc::clone(&req_counter);
        tokio::spawn(async move {
            let id = counter.fetch_add(1, Ordering::SeqCst);
            if let Err(e) = handle_connection(stream, client).await {
                log::debug!("Browser proxy request {} error: {}", id, e);
            }
        });
    }
}

/// Handle a single HTTP connection.
async fn handle_connection(
    stream: tokio::net::TcpStream,
    client: reqwest::Client,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (mut read_half, mut write_half) = tokio::io::split(stream);
    let mut buf = vec![0u8; 8192];

    // Read the first part of the request
    let n = read_half.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }

    let request_text = String::from_utf8_lossy(&buf[..n]);
    let first_line = request_text.lines().next().unwrap_or("");

    // Parse the query string to extract the url parameter
    let target_url = extract_url_param(&request_text, first_line);

    if let Some(target) = target_url {
        match client.get(&target).send().await {
            Ok(response) => {
                let content_type = response
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("text/html")
                    .to_string();
                let status = response.status();
                let bytes = response.bytes().await?;

                let body = if content_type.contains("text/html") {
                    rewrite_html(&bytes, &target)
                } else {
                    bytes.to_vec()
                };

                let headers = format!(
                    "HTTP/1.1 {} OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    status.as_u16(),
                    content_type,
                    body.len()
                );
                write_half.write_all(headers.as_bytes()).await?;
                write_half.write_all(&body).await?;
            }
            Err(e) => {
                let body = format!("Proxy error: {}", e);
                let response = format!(
                    "HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                write_half.write_all(response.as_bytes()).await?;
            }
        }
    } else {
        let body = "Browser proxy active. Use /proxy?url=<target_url>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        write_half.write_all(response.as_bytes()).await?;
    }

    Ok(())
}

/// Extract the `url` query parameter from the raw HTTP request.
fn extract_url_param(request: &str, first_line: &str) -> Option<String> {
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    let path = parts[1];
    let url_start = path.find("?url=")?;
    let encoded = &path[url_start + 5..];
    // URL decode
    Some(url_unescape(encoded))
}

/// Simple URL percent-decoder.
fn url_unescape(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '+' => result.push(' '),
            '%' => {
                let high = chars.next();
                let low = chars.next();
                match (high, low) {
                    (Some(h), Some(l)) => {
                        let hex_str = format!("{}{}", h, l);
                        if let Ok(byte_val) = u8::from_str_radix(&hex_str, 16) {
                            result.push(byte_val as char);
                        }
                    }
                    _ => {}
                }
            }
            _ => result.push(ch),
        }
    }
    result
}

/// Rewrite HTML content to route links through the proxy.
fn rewrite_html(bytes: &[u8], base_url: &str) -> Vec<u8> {
    let html = String::from_utf8_lossy(bytes);
    let base_tag = format!(r#"<base href="{}">"#, base_url);

    // Inject <base> tag after <head> or at the start
    let mut modified = if let Some(head_end) = html.to_lowercase().find("<head>") {
        let insert_pos = head_end + 6;
        let mut result = html[..insert_pos].to_string();
        result.push_str(&base_tag);
        result.push_str(&html[insert_pos..]);
        result
    } else if let Some(html_tag) = html.to_lowercase().find("<html") {
        if let Some(pos) = html[html_tag..].find('>') {
            let end_pos = html_tag + pos + 1;
            let mut result = html[..end_pos].to_string();
            result.push_str("<head>");
            result.push_str(&base_tag);
            result.push_str("</head>");
            result.push_str(&html[end_pos..]);
            result
        } else {
            html.to_string()
        }
    } else {
        let mut result = String::new();
        result.push_str("<html><head>");
        result.push_str(&base_tag);
        result.push_str("</head>");
        result.push_str(&html);
        result.push_str("</html>");
        result
    };

    // Inject a script to intercept link clicks and route through proxy
    let script = r#"
<script>
(function() {
    var proxyPort = window.location.port;
    function toProxy(url) {
        if (!url || url.startsWith('javascript:') || url.startsWith('data:') || url.startsWith('#') || url.startsWith('mailto:') || url.startsWith('tel:')) return url;
        if (url.startsWith('http://') || url.startsWith('https://')) {
            if (url.startsWith('http://localhost:') && url.includes('/proxy?url=')) return url;
            return 'http://localhost:' + proxyPort + '/proxy?url=' + encodeURIComponent(url);
        }
        return url;
    }
    document.addEventListener('click', function(e) {
        var el = e.target.closest('a');
        if (el && el.href && !el.href.startsWith('http://localhost:')) {
            var newUrl = toProxy(el.href);
            if (newUrl !== el.href) {
                e.preventDefault();
                window.location.href = newUrl;
            }
        }
    });
})();
</script>
"#;

    if let Some(body_tag) = modified.to_lowercase().find("<body>") {
        let insert_pos = body_tag + 6;
        modified.insert_str(insert_pos, script);
    } else if let Some(body_tag) = modified.to_lowercase().find("<body ") {
        if let Some(pos) = modified[body_tag..].find('>') {
            let insert_pos = body_tag + pos + 1;
            modified.insert_str(insert_pos, script);
        }
    }

    // Strip reCAPTCHA / captcha scripts — they fail on localhost origin anyway
    // and block the entire page load with "Localhost is not in the list of
    // supported domains for this site key" errors.
    modified = strip_script_tag(&modified, "recaptcha");
    modified = strip_script_tag(&modified, "hcaptcha");
    modified = strip_script_tag(&modified, "turnstile");
    modified = strip_script_tag(&modified, "captcha");

    modified.into_bytes()
}

/// Strip all `<script ...keyword...>...</script>` tags from HTML (case-insensitive).
fn strip_script_tag(html: &str, keyword: &str) -> String {
    let lower_html = html.to_lowercase();
    let lower_kw = keyword.to_lowercase();
    let mut result = html.to_string();
    let mut start = 0;
    while let Some(pos) = lower_html[start..].find("<script") {
        let script_start = start + pos;
        // Find the end of this script tag
        if let Some(end_pos) = lower_html[script_start..].find("</script>") {
            let script_end = script_start + end_pos + 9; // +9 for "</script>"
            let script_tag = &lower_html[script_start..script_end];
            if script_tag.contains(&lower_kw) {
                // Remove the script tag from the original (case-sensitive) string
                result.replace_range(script_start..script_end, "");
                // Rebuild for next iteration
                return strip_script_tag(&result, keyword);
            }
            start = script_end;
        } else {
            break;
        }
    }
    result
}
