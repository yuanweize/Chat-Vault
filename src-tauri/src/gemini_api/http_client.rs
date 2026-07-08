//! HTTP 客户端封装：reqwest Client 构建、cookie 注入、请求间延迟、重试。

use std::collections::HashMap;

use std::sync::atomic::Ordering;

use rand::distr::{Distribution, Uniform};

use crate::browser_info;
use crate::protocol::{REQUEST_DELAY, REQUEST_JITTER_MAX, REQUEST_JITTER_MIN};

use super::GeminiExporter;

// ============================================================================
// Client 构建
// ============================================================================

/// 构建 reqwest 异步客户端，注入 cookie 和默认 headers。
pub fn build_http_client(cookies: &HashMap<String, String>) -> reqwest::Client {
    let cookie_header: String = cookies
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("; ");

    let mut headers = reqwest::header::HeaderMap::new();
    if let Ok(val) = reqwest::header::HeaderValue::from_str(browser_info::build_user_agent()) {
        headers.insert(reqwest::header::USER_AGENT, val);
    }
    if let Ok(val) = reqwest::header::HeaderValue::from_str(browser_info::detect_accept_language())
    {
        headers.insert(reqwest::header::ACCEPT_LANGUAGE, val);
    }
    // sec-ch-ua 系列
    if let Ok(val) = reqwest::header::HeaderValue::from_str(browser_info::build_sec_ch_ua()) {
        headers.insert("sec-ch-ua", val);
    }
    headers.insert(
        "sec-ch-ua-mobile",
        reqwest::header::HeaderValue::from_static("?0"),
    );
    if let Ok(val) = reqwest::header::HeaderValue::from_str(browser_info::platform_hint()) {
        headers.insert("sec-ch-ua-platform", val);
    }
    // accept-encoding 由 reqwest 的 gzip/brotli/deflate/zstd feature 自动处理
    if !cookie_header.is_empty() {
        if let Ok(val) = reqwest::header::HeaderValue::from_str(&cookie_header) {
            headers.insert(reqwest::header::COOKIE, val);
        }
    }

    reqwest::Client::builder()
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("Failed to build reqwest client")
}

// ============================================================================
// 延迟 / 重试
// ============================================================================

/// 三角分布随机抖动（模拟 Python random.triangular）
fn triangular_jitter(min: f64, max: f64, mode: f64) -> f64 {
    let u: f64 = Uniform::new(0.0_f64, 1.0_f64)
        .expect("invalid uniform range")
        .sample(&mut rand::rng());
    let fc = (mode - min) / (max - min);
    if u < fc {
        min + ((max - min) * (mode - min) * u).sqrt()
    } else {
        max - ((max - min) * (max - mode) * (1.0 - u)).sqrt()
    }
}

impl GeminiExporter {
    /// 请求前等待：请求间延迟 + 抖动。
    pub async fn before_request(&self, _label: &str) -> Result<(), String> {
        // 检查取消
        if self.cancelled.load(Ordering::Relaxed) {
            return Err("用户取消".to_string());
        }

        // 首次请求不加延迟
        if self.request_started.load(Ordering::Relaxed) {
            let jitter = triangular_jitter(
                REQUEST_JITTER_MIN,
                REQUEST_JITTER_MAX,
                crate::protocol::REQUEST_JITTER_MODE,
            );
            let delay_sec = REQUEST_DELAY + jitter;
            tokio::time::sleep(std::time::Duration::from_secs_f64(delay_sec)).await;
        }

        self.request_started.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// GET 请求 + 自动重试（最多 attempts 次）。
    ///
    /// `extra_headers`：额外附加的请求 headers（如导航专用 headers）。
    pub async fn client_get_with_retry(
        &self,
        url: &str,
        params: &[(&str, String)],
        attempts: u32,
        extra_headers: &[(&str, &str)],
    ) -> Result<reqwest::Response, String> {
        let mut last_err = String::new();
        for _ in 0..attempts {
            self.before_request("http_get").await?;

            let mut req = self.client.get(url);
            if !params.is_empty() {
                req = req.query(params);
            }
            for &(name, value) in extra_headers {
                if let Ok(val) = reqwest::header::HeaderValue::from_str(value) {
                    req = req.header(name, val);
                }
            }

            match req.send().await {
                Ok(resp) => {
                    return Ok(resp);
                }
                Err(e) => {
                    last_err = e.to_string();
                }
            }
        }
        Err(last_err)
    }
}
