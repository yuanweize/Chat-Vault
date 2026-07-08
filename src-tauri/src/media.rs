//! Gemini 媒体工具：媒体类型推断、URL 辅助函数。

use std::path::Path;
use url::Url;

// ============================================================================
// 常量
// ============================================================================

pub const PROTECTED_MEDIA_HOSTS: &[&str] = &[
    "lh3.google.com",
    "lh3.googleusercontent.com",
    "contribution.usercontent.google.com",
];

// ============================================================================
// 媒体类型推断
// ============================================================================

/// 根据文件名/扩展名推断媒体类型
pub fn infer_media_type(media_hint: &str) -> &'static str {
    if media_hint.is_empty() {
        return "file";
    }
    let ext = Path::new(media_hint)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "avif" | "heic" | "heif" | "svg" => {
            "image"
        }
        "mp4" | "mov" | "webm" | "mkv" | "m4v" | "avi" | "3gp" => "video",
        "mp3" | "m4a" | "wav" | "aac" | "flac" | "ogg" | "opus" | "wma" | "aiff" => "audio",
        _ => "file",
    }
}

// ============================================================================
// URL 工具
// ============================================================================

/// 媒体日志字段
pub struct MediaLogFields {
    pub media: String,
    pub domain: String,
}

/// 从 URL 和类型信息构建日志字段
pub fn media_log_fields(
    url_text: Option<&str>,
    media_type: Option<&str>,
    media_hint: Option<&str>,
) -> MediaLogFields {
    let domain = url_text
        .and_then(|u| Url::parse(u).ok())
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
        .unwrap_or_else(|| "-".to_string());

    let kind = match media_type {
        Some(t) if matches!(t, "image" | "video" | "file") => t.to_string(),
        _ => infer_media_type(media_hint.unwrap_or("")).to_string(),
    };

    MediaLogFields {
        media: kind,
        domain,
    }
}

/// 为 URL 附加 authuser 查询参数
pub fn append_authuser(url_str: &str, authuser: &str) -> String {
    let mut parsed = match Url::parse(url_str) {
        Ok(u) => u,
        Err(_) => return url_str.to_string(),
    };
    // Remove existing authuser if any, then append
    let pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(k, _)| k != "authuser")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    parsed.query_pairs_mut().clear();
    for (k, v) in &pairs {
        parsed.query_pairs_mut().append_pair(k, v);
    }
    parsed.query_pairs_mut().append_pair("authuser", authuser);
    parsed.to_string()
}

/// 检查 URL 是否属于受保护的媒体域名
pub fn is_protected_media_url(url_text: &str) -> bool {
    let host = match Url::parse(url_text) {
        Ok(u) => u.host_str().unwrap_or("").to_lowercase(),
        Err(_) => return false,
    };
    PROTECTED_MEDIA_HOSTS.iter().any(|&h| host == h)
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_media_type() {
        assert_eq!(infer_media_type("photo.jpg"), "image");
        assert_eq!(infer_media_type("video.mp4"), "video");
        assert_eq!(infer_media_type("song.mp3"), "audio");
        assert_eq!(infer_media_type("doc.pdf"), "file");
        assert_eq!(infer_media_type(""), "file");
    }

    #[test]
    fn test_append_authuser() {
        let url = "https://lh3.google.com/path?key=val";
        let result = append_authuser(url, "2");
        assert!(result.contains("authuser=2"));
        assert!(result.contains("key=val"));
    }

    #[test]
    fn test_append_authuser_replaces_existing() {
        let url = "https://lh3.google.com/path?authuser=0&key=val";
        let result = append_authuser(url, "3");
        assert!(result.contains("authuser=3"));
        assert!(!result.contains("authuser=0"));
    }

    #[test]
    fn test_is_protected_media_url() {
        assert!(is_protected_media_url(
            "https://lh3.googleusercontent.com/img.jpg"
        ));
        assert!(is_protected_media_url("https://lh3.google.com/media/abc"));
        assert!(!is_protected_media_url("https://example.com/img.jpg"));
    }

    #[test]
    fn test_media_log_fields() {
        let fields = media_log_fields(Some("https://lh3.google.com/img.jpg"), Some("image"), None);
        assert_eq!(fields.media, "image");
        assert_eq!(fields.domain, "lh3.google.com");
    }

    #[test]
    fn test_media_log_fields_infer() {
        let fields = media_log_fields(None, None, Some("clip.mp4"));
        assert_eq!(fields.media, "video");
        assert_eq!(fields.domain, "-");
    }
}
