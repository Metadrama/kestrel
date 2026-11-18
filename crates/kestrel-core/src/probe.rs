use kestrel_proto::{KestrelError, MimeCategory};
use percent_encoding::percent_decode_str;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE, ETAG, LAST_MODIFIED, RANGE};
use reqwest::Client;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub url: String,
    pub final_url: String,
    pub content_length: Option<u64>,
    pub accept_ranges: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub filename: String,
    pub mime_category: MimeCategory,
    pub content_type: Option<String>,
}

pub struct HttpProber {
    client: Client,
}

impl HttpProber {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub async fn probe(&self, url: &str, custom_headers: &[(String, String)]) -> Result<ProbeResult, KestrelError> {
        let mut req_headers = HeaderMap::new();
        for (k, v) in custom_headers {
            if let (Ok(hname), Ok(hval)) = (k.parse::<reqwest::header::HeaderName>(), HeaderValue::from_str(v)) {
                req_headers.insert(hname, hval);
            }
        }

        // Try HEAD first
        let head_res = self.client.head(url)
            .headers(req_headers.clone())
            .send()
            .await;

        let response = match head_res {
            Ok(res) if res.status().is_success() => res,
            _ => {
                // Fallback to GET with Range: bytes=0-0 (handles 405 Method Not Allowed or HEAD failure)
                let mut range_headers = req_headers.clone();
                range_headers.insert(RANGE, HeaderValue::from_static("bytes=0-0"));

                self.client.get(url)
                    .headers(range_headers)
                    .send()
                    .await
                    .map_err(|e| KestrelError::Network(format!("Probe request failed: {}", e)))?
            }
        };

        let final_url = response.url().to_string();
        let headers = response.headers();

        // 1. Content-Length & Accept-Ranges
        let mut content_length = None;
        if let Some(cl) = headers.get(CONTENT_LENGTH) {
            if let Ok(cl_str) = cl.to_str() {
                if let Ok(val) = cl_str.trim().parse::<u64>() {
                    content_length = Some(val);
                }
            }
        }

        // If 206 Partial Content was returned, Content-Range gives total size
        if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            if let Some(cr) = headers.get(reqwest::header::CONTENT_RANGE) {
                if let Ok(cr_str) = cr.to_str() {
                    // bytes 0-0/1234567
                    if let Some(slash_idx) = cr_str.rfind('/') {
                        let total_part = &cr_str[slash_idx + 1..];
                        if let Ok(tot) = total_part.trim().parse::<u64>() {
                            content_length = Some(tot);
                        }
                    }
                }
            }
        }

        let accept_ranges = headers.get(ACCEPT_RANGES)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.eq_ignore_ascii_case("bytes"))
            .unwrap_or(response.status() == reqwest::StatusCode::PARTIAL_CONTENT);

        let etag = headers.get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim_matches('"').to_string());

        let last_modified = headers.get(LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let content_type = headers.get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(';').next().unwrap_or(s).trim().to_string());

        // 2. Filename resolution (RFC 5987 / Content-Disposition / URL fallback)
        let filename = extract_filename(headers.get(CONTENT_DISPOSITION), &final_url);

        let ext = Path::new(&filename)
            .extension()
            .and_then(|e| e.to_str());

        let mime_category = MimeCategory::from_mime_or_ext(content_type.as_deref(), ext);

        Ok(ProbeResult {
            url: url.to_string(),
            final_url,
            content_length,
            accept_ranges,
            etag,
            last_modified,
            filename,
            mime_category,
            content_type,
        })
    }
}

pub fn extract_filename(disposition: Option<&HeaderValue>, url_str: &str) -> String {
    if let Some(val) = disposition {
        if let Ok(disp_str) = val.to_str() {
            // RFC 5987 filename*=UTF-8''encoded_name
            if let Some(pos) = disp_str.find("filename*=") {
                let rest = &disp_str[pos + 10..];
                let enc_part = rest.split(';').next().unwrap_or(rest).trim().trim_matches('"');
                if let Some(idx) = enc_part.find("''") {
                    let encoded = &enc_part[idx + 2..];
                    if let Ok(decoded) = percent_decode_str(encoded).decode_utf8() {
                        let name = sanitize_filename(&decoded);
                        if !name.is_empty() {
                            return name;
                        }
                    }
                }
            }

            // Standard filename="name"
            if let Some(pos) = disp_str.find("filename=") {
                let rest = &disp_str[pos + 9..];
                let raw_name = rest.split(';').next().unwrap_or(rest).trim().trim_matches('"');
                let name = sanitize_filename(raw_name);
                if !name.is_empty() {
                    return name;
                }
            }
        }
    }

    // Fallback: extract last segment of URL path
    if let Ok(parsed) = url::Url::parse(url_str) {
        let segments: Vec<&str> = parsed.path_segments()
            .map(|c| c.collect())
            .unwrap_or_default();

        if let Some(last) = segments.last() {
            if let Ok(decoded) = percent_decode_str(last).decode_utf8() {
                let name = sanitize_filename(&decoded);
                if !name.is_empty() && name != "/" {
                    return name;
                }
            }
        }
    }

    "download".to_string()
}

pub fn sanitize_filename(name: &str) -> String {
    let forbidden = ['/', '\\', '?', '%', '*', ':', '|', '"', '<', '>', '\0', '\n', '\r'];
    let cleaned: String = name.chars()
        .map(|c| if forbidden.contains(&c) { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.');
    if trimmed.is_empty() {
        "file".to_string()
    } else {
        trimmed.to_string()
    }
}
