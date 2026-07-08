use crate::str_err::ToStringErr;
use anyhow::Result;
use serde_json::Value;
use std::fs;
use std::path::Path;

pub fn generate_markdown_from_rows(rows: &[Value], markdown_file: &Path) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }

    let meta = match rows.first() {
        Some(m) if m.get("type").and_then(|v| v.as_str()) == Some("meta") => m,
        _ => return Err(anyhow::anyhow!("First row must be meta")),
    };

    let title = meta
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Untitled");
    let created_at = meta.get("createdAt").and_then(|v| v.as_str()).unwrap_or("");
    let updated_at = meta.get("updatedAt").and_then(|v| v.as_str()).unwrap_or("");

    let mut md = String::new();
    md.push_str(&format!("# {}\n\n", title));
    md.push_str(&format!("*Created At: {}*\n", created_at));
    md.push_str(&format!("*Updated At: {}*\n\n", updated_at));
    md.push_str("---\n\n");

    for row in rows.iter().skip(1) {
        if row.get("type").and_then(|v| v.as_str()) != Some("message") {
            continue;
        }

        let role = row
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let text = row.get("text").and_then(|v| v.as_str()).unwrap_or("");

        let role_header = match role {
            "user" => "### 🧑 User",
            "model" => "### 🤖 Model",
            _ => "### 💬 Unknown",
        };

        md.push_str(&format!("{}\n\n", role_header));

        // Thinking (if model)
        if let Some(thinking) = row.get("thinking").and_then(|v| v.as_str()) {
            if !thinking.is_empty() {
                md.push_str("> **Thinking**:\n");
                for line in thinking.lines() {
                    md.push_str(&format!("> {}\n", line));
                }
                md.push_str("\n");
            }
        }

        // Text content
        if !text.is_empty() {
            md.push_str(&format!("{}\n\n", text));
        }

        // Attachments
        if let Some(attachments) = row.get("attachments").and_then(|v| v.as_array()) {
            if !attachments.is_empty() {
                md.push_str("**Attachments:**\n");
                for att in attachments {
                    let att_type = att.get("type").and_then(|v| v.as_str()).unwrap_or("file");
                    let name = att.get("name").and_then(|v| v.as_str()).unwrap_or("file");
                    let media_id = att.get("media_id").and_then(|v| v.as_str()).unwrap_or("");

                    // Using relative path to point to media folder.
                    // Markdown files are in `exports/markdown/`
                    // Media files are in `media/`
                    // So relative path is `../../media/<media_id>`
                    let rel_path = format!("../../media/{}", media_id);

                    if att_type.starts_with("image") {
                        md.push_str(&format!("![{}]({})\n", name, rel_path));
                    } else if att_type.starts_with("video") || att_type.starts_with("audio") {
                        // Markdown doesn't embed video nicely without HTML, but we can provide a link
                        md.push_str(&format!("[{}]({})\n", name, rel_path));
                    } else {
                        md.push_str(&format!("[{}]({})\n", name, rel_path));
                    }
                }
                md.push_str("\n");
            }
        }

        md.push_str("---\n\n");
    }

    if let Some(parent) = markdown_file.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(markdown_file, md)?;
    Ok(())
}
