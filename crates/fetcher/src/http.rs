//! Small HTTP helpers shared by the platform fetchers.
//!
//! The project deliberately uses a blocking client: every fetch runs on a
//! worker thread, so a synchronous API keeps cancellation and progress logic
//! straightforward and avoids a second async runtime next to GPUI.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;
use url::Url;

use crate::{FetchError, Result};

pub const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

pub fn agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .user_agent(USER_AGENT)
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_global(None)
        .build();
    ureq::Agent::new_with_config(config)
}

pub fn get_json(agent: &ureq::Agent, url: &str, headers: &[(&str, &str)]) -> Result<Value> {
    let mut request = agent.get(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let mut response = request
        .call()
        .map_err(|error| FetchError::Network(error.to_string()))?;
    let status = response.status();
    let text = response
        .body_mut()
        .with_config()
        .limit(32 * 1024 * 1024)
        .read_to_string()
        .map_err(|error| FetchError::Network(error.to_string()))?;
    if !status.is_success() {
        return Err(FetchError::Api(format!(
            "{} returned HTTP {}: {}",
            host_label(url),
            status.as_u16(),
            truncate(&text, 300)
        )));
    }
    serde_json::from_str(&text).map_err(|error| {
        FetchError::Parse(format!("Invalid JSON from {}: {error}", host_label(url)))
    })
}

pub fn get_bytes(
    agent: &ureq::Agent,
    url: &str,
    headers: &[(&str, &str)],
    limit: u64,
) -> Result<Vec<u8>> {
    let mut request = agent.get(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let mut response = request
        .call()
        .map_err(|error| FetchError::Network(error.to_string()))?;
    let status = response.status();
    let mut bytes = Vec::new();
    response
        .body_mut()
        .with_config()
        .limit(limit)
        .reader()
        .read_to_end(&mut bytes)
        .map_err(|error| FetchError::Network(error.to_string()))?;
    if !status.is_success() {
        return Err(FetchError::Api(format!(
            "{} returned HTTP {}",
            host_label(url),
            status.as_u16()
        )));
    }
    Ok(bytes)
}

pub fn get_text(
    agent: &ureq::Agent,
    url: &str,
    headers: &[(&str, &str)],
    limit: u64,
) -> Result<String> {
    let mut request = agent.get(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let mut response = request
        .call()
        .map_err(|error| FetchError::Network(error.to_string()))?;
    let status = response.status();
    let text = response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_string()
        .map_err(|error| FetchError::Network(error.to_string()))?;
    if !status.is_success() {
        return Err(FetchError::Api(format!(
            "{} returned HTTP {}",
            host_label(url),
            status.as_u16()
        )));
    }
    Ok(text)
}

/// Download to a file while reporting bytes, total and instantaneous speed.
pub fn download_to(
    agent: &ureq::Agent,
    url: &str,
    headers: &[(&str, &str)],
    destination: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, Option<u64>, u64),
) -> Result<u64> {
    if cancel.load(Ordering::Relaxed) {
        return Err(FetchError::Canceled);
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| FetchError::Io(error.to_string()))?;
    }
    let mut request = agent.get(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let mut response = request
        .call()
        .map_err(|error| FetchError::Network(error.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        let mut body = String::new();
        let _ = response
            .body_mut()
            .with_config()
            .limit(4096)
            .reader()
            .read_to_string(&mut body);
        return Err(FetchError::Api(format!(
            "Download failed with HTTP {}: {}",
            status.as_u16(),
            truncate(&body, 240)
        )));
    }
    let total = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let mut file = File::create(destination).map_err(|error| FetchError::Io(error.to_string()))?;
    let mut reader = response.body_mut().with_config().reader();
    let mut buffer = vec![0u8; 128 * 1024];
    let mut downloaded = 0u64;
    let mut window_start = Instant::now();
    let mut window_bytes = 0u64;
    progress(0, total, 0);
    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(destination);
            return Err(FetchError::Canceled);
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|error| FetchError::Network(error.to_string()))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|error| FetchError::Io(error.to_string()))?;
        downloaded += read as u64;
        window_bytes += read as u64;
        let elapsed = window_start.elapsed();
        let speed = if elapsed >= Duration::from_millis(400) {
            let speed = (window_bytes as f64 / elapsed.as_secs_f64()) as u64;
            window_start = Instant::now();
            window_bytes = 0;
            speed
        } else {
            0
        };
        progress(downloaded, total, speed);
    }
    file.flush()
        .map_err(|error| FetchError::Io(error.to_string()))?;
    Ok(downloaded)
}

/// Resolve a shortened link without downloading the final page.
pub fn resolve_redirect(url: &str) -> Result<String> {
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .user_agent(USER_AGENT)
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(30)))
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut current = url.to_string();
    for _ in 0..4 {
        let response = agent
            .get(&current)
            .call()
            .map_err(|error| FetchError::Network(error.to_string()))?;
        let status = response.status();
        if status.is_redirection()
            && let Some(location) = response.headers().get("location")
        {
            let location = location
                .to_str()
                .map_err(|error| FetchError::Network(error.to_string()))?;
            current = Url::parse(&current)
                .and_then(|base| base.join(location))
                .map_err(|error| FetchError::Network(error.to_string()))?
                .to_string();
            continue;
        }
        return Ok(current);
    }
    Err(FetchError::Network("Too many redirects".into()))
}

/// Read a Netscape cookie file and produce a Cookie header for a domain.
/// Values are never logged.
pub fn cookie_header(path: &Path, domain_suffix: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut pairs = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 7 {
            continue;
        }
        let domain = fields[0].trim_start_matches('.');
        if !domain.eq_ignore_ascii_case(domain_suffix)
            && !domain.ends_with(&format!(".{domain_suffix}"))
        {
            continue;
        }
        let name = fields[5].trim();
        let value = fields[6].trim();
        if !name.is_empty() {
            pairs.push(format!("{name}={value}"));
        }
    }
    if pairs.is_empty() {
        None
    } else {
        Some(pairs.join("; "))
    }
}

/// Try to read the social preview image from a channel page.
pub fn og_image(html: &str) -> Option<String> {
    let patterns = [
        r#"property="og:image" content="([^"]+)""#,
        r#"content="([^"]+)" property="og:image""#,
        r#""avatarUrl":"([^"]+)""#,
        r#""avatar":{"thumbnails":\[\{"url":"([^"]+)""#,
    ];
    for pattern in patterns {
        if let Ok(regex) = regex::Regex::new(pattern)
            && let Some(captures) = regex.captures(html)
            && let Some(value) = captures.get(1)
        {
            return Some(value.as_str().replace("\\u0026", "&").replace("\\/", "/"));
        }
    }
    None
}

pub fn absolute_media_url(value: &str) -> String {
    if value.starts_with("//") {
        format!("https:{value}")
    } else if value.starts_with("http://") {
        value.replacen("http://", "https://", 1)
    } else {
        value.to_string()
    }
}

fn host_label(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| "remote server".to_string())
}

fn truncate(value: &str, max: usize) -> String {
    let value = value.trim();
    if value.chars().count() <= max {
        value.to_string()
    } else {
        let mut short: String = value.chars().take(max.saturating_sub(1)).collect();
        short.push('…');
        short
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_netscape_cookies_for_the_matching_domain_only() {
        let directory =
            std::env::temp_dir().join(format!("videleum-cookie-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("cookies.txt");
        std::fs::write(
            &path,
            "# Netscape HTTP Cookie File\n.bilibili.com\tTRUE\t/\tTRUE\t0\tSESSDATA\tabc\n.example.com\tTRUE\t/\tFALSE\t0\tOTHER\txyz\n",
        )
        .unwrap();
        let header = cookie_header(&path, "bilibili.com").unwrap();
        assert_eq!(header, "SESSDATA=abc");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn extracts_og_image() {
        let html = r#"<meta property="og:image" content="https://example.com/a.jpg">"#;
        assert_eq!(og_image(html).as_deref(), Some("https://example.com/a.jpg"));
    }
}
