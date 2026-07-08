//! Google batchexecute RPC 请求构造 + 响应解析。
//!
//! 对应 Python GeminiExporter._batchexecute 方法。
//! - 构建 f.req / at / bl / fsid / reqid 参数
//! - POST application/x-www-form-urlencoded
//! - 响应分发到指定 rpcid
//! - session 过期检测
//! - 非 session 错误自动重试一次

use std::sync::atomic::Ordering;

use crate::browser_info;
use crate::protocol::{
    has_batchexecute_session_error, parse_batchexecute_response, ProtocolError, GEMINI_BASE,
    REQUEST_RETRY_PAUSE_SECONDS,
};

use super::GeminiExporter;

impl GeminiExporter {
    /// 递增并返回下一个 reqid
    fn next_reqid(&self) -> String {
        let val = self.reqid.fetch_add(100000, Ordering::Relaxed);
        (val + 100000).to_string()
    }

    /// 发送 batchexecute 请求，返回指定 rpcid 的解析数据。
    ///
    /// 非 session 错误会自动暂停后重试一次。
    ///
    /// # Arguments
    /// - `rpcid`: 目标 RPC 标识（如 "MaZiqc"、"hNvQHb"）
    /// - `payload_json`: JSON 序列化后的 payload 字符串
    /// - `source_path`: 来源路径（用于 source-path 参数，可为空）
    pub async fn batchexecute(
        &self,
        rpcid: &str,
        payload_json: &str,
        source_path: &str,
    ) -> Result<serde_json::Value, String> {
        match self
            .batchexecute_once(rpcid, payload_json, source_path)
            .await
        {
            Ok(data) => Ok(data),
            Err(e) => {
                // session 过期不在此层重试，交给上层 run_with_retry 重建 exporter
                if e.contains("HTTP 200 但响应数据为空") {
                    return Err(e);
                }
                // 用户取消不重试
                if e.contains("用户取消") {
                    return Err(e);
                }
                log::warn!(
                    "[batchexecute] 失败，{}s 后重试一次: {}",
                    REQUEST_RETRY_PAUSE_SECONDS,
                    e
                );
                tokio::time::sleep(std::time::Duration::from_secs_f64(
                    REQUEST_RETRY_PAUSE_SECONDS,
                ))
                .await;
                self.batchexecute_once(rpcid, payload_json, source_path)
                    .await
            }
        }
    }

    /// 发送单次 batchexecute 请求（不含重试逻辑）。
    async fn batchexecute_once(
        &self,
        rpcid: &str,
        payload_json: &str,
        source_path: &str,
    ) -> Result<serde_json::Value, String> {
        let f_req = serde_json::to_string(&serde_json::json!([[[
            rpcid,
            payload_json,
            serde_json::Value::Null,
            "generic"
        ]]]))
        .map_err(|e| format!("序列化 f.req 失败: {}", e))?;

        let at = self
            .at
            .as_deref()
            .ok_or_else(|| "未初始化认证参数 (at)".to_string())?;
        let bl = self
            .bl
            .as_deref()
            .ok_or_else(|| "未初始化认证参数 (bl)".to_string())?;
        let fsid = self.fsid.as_deref().unwrap_or("0");

        // 构建查询参数
        let reqid_val = self.next_reqid();
        let mut query_params: Vec<(&str, String)> = vec![
            ("rpcids", rpcid.to_string()),
            ("bl", bl.to_string()),
            ("f.sid", fsid.to_string()),
            ("hl", "zh-CN".to_string()),
            ("_reqid", reqid_val),
            ("rt", "c".to_string()),
        ];

        // 添加 authuser
        for (k, v) in self.authuser_params() {
            query_params.push((k, v));
        }

        if !source_path.is_empty() {
            query_params.push(("source-path", source_path.to_string()));
        }

        // 构建 form body
        let form_body = [("f.req", f_req.as_str()), ("at", at)];

        let url = format!("{}/_/BardChatUi/data/batchexecute", GEMINI_BASE);

        // 执行前等待
        self.before_request(&format!("batchexecute:{}", rpcid))
            .await?;

        // 发送 POST 请求
        let resp = self
            .client
            .post(&url)
            .query(&query_params)
            .form(&form_body)
            .header(
                "Content-Type",
                "application/x-www-form-urlencoded;charset=UTF-8",
            )
            .header("accept", browser_info::API_ACCEPT)
            .header("sec-fetch-dest", browser_info::API_SEC_FETCH_DEST)
            .header("sec-fetch-mode", browser_info::API_SEC_FETCH_MODE)
            .header("sec-fetch-site", browser_info::API_SEC_FETCH_SITE)
            .header("origin", browser_info::API_ORIGIN)
            .header("referer", browser_info::API_REFERER)
            .header("x-goog-ext-73010989-jspb", "[0]")
            .header(
                "x-goog-ext-525001261-jspb",
                "[1,null,null,null,null,null,null,null,[4]]",
            )
            .send()
            .await
            .map_err(|e| format!("batchexecute 网络请求失败: {}", e))?;

        let status = resp.status();
        let resp_text = resp
            .text()
            .await
            .map_err(|e| format!("读取 batchexecute 响应失败: {}", e))?;

        if !status.is_success() {
            log::debug!(
                "HTTP {} 响应失败, body length={}",
                status.as_u16(),
                resp_text.len()
            );
            return Err(format!("batchexecute 失败: HTTP {}", status.as_u16()));
        }

        let results = parse_batchexecute_response(&resp_text);
        for (rid, data) in &results {
            if rid == rpcid {
                return Ok(data.clone());
            }
        }

        // 未找到目标 rpcid
        if has_batchexecute_session_error(&resp_text, rpcid) {
            return Err(format!(
                "{}: rpcid={}",
                ProtocolError::SessionExpired,
                rpcid
            ));
        }

        Err(format!("响应中未找到 {} 数据", rpcid))
    }
}
