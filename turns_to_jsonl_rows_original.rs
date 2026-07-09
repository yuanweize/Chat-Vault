pub fn turns_to_jsonl_rows(
    parsed_turns: &[Value],
    conv_id: &str,
    account_id: &str,
    title: &str,
    chat_info: &Value,
    media_dir: &Path,
) -> Vec<Value> {
    let now_iso = Utc::now().to_rfc3339();
    let bare_id = crate::protocol::strip_c_prefix(conv_id);
    let ordered_turns = sort_parsed_turns_by_timestamp(parsed_turns);

    let ts_list: Vec<i64> = ordered_turns
        .iter()
        .filter_map(|t| t.as_object()?.get("timestamp")?.as_i64())
        .collect();
    let created_at_ts = ts_list.iter().copied().min();

    let chat_obj = chat_info.as_object();
    let remote_ts = chat_obj
        .and_then(|o| o.get("latest_update_ts"))
        .and_then(|v| coerce_epoch_seconds(v))
        .or_else(|| ts_list.iter().copied().max());

    let updated_at = to_iso_utc(remote_ts).or_else(|| {
        chat_obj
            .and_then(|o| o.get("latest_update_iso"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
    });

    let created_at = to_iso_utc(created_at_ts)
        .or_else(|| updated_at.clone())
        .unwrap_or_else(|| now_iso.clone());

    let remote_hash = remote_ts.map(|ts| ts.to_string());

    let mut rows = vec![json!({
        "type": "meta",
        "id": bare_id,
        "accountId": account_id,
        "title": title,
        "createdAt": created_at,
        "updatedAt": updated_at,
        "remoteHash": remote_hash,
    })];

    for turn in &ordered_turns {
        let turn_obj = match turn.as_object() {
            Some(o) => o,
            None => continue,
        };
        let turn_id = turn_obj
            .get("turn_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| Uuid::new_v4().to_string().replace("-", ""));

        let ts = turn_obj
            .get("timestamp")
            .and_then(|v| v.as_i64())
            .and_then(|t| to_iso_utc(Some(t)))
            .unwrap_or_else(|| now_iso.clone());

        // User message
        let user = turn_obj.get("user").and_then(|v| v.as_object());
        let user_text = user
            .and_then(|u| u.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let user_attachments = build_attachments(user.and_then(|u| u.get("files")));

        rows.push(json!({
            "type": "message",
            "id": format!("{}_u", turn_id),
            "role": "user",
            "text": user_text,
            "attachments": user_attachments,
            "timestamp": ts,
        }));

        // Assistant message
        let asst = turn_obj.get("assistant").and_then(|v| v.as_object());
        let asst_text = asst
            .and_then(|a| a.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let asst_attachments = build_attachments(asst.and_then(|a| a.get("files")));
        let model = asst
            .and_then(|a| a.get("model"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Deep Research report turn：assistant 行用 ai[12][8]["57"][0][5] 的报告完成时间
        // （user 行保持原 turn[4][0]，不受影响）。字段缺失时回落到 turn ts。
        let model_ts = asst
            .and_then(|a| a.get("deep_research"))
            .and_then(|dr| {
                if dr.get("type").and_then(|v| v.as_str()) == Some("report") {
                    dr.get("completion_ts").and_then(|v| v.as_i64())
                } else {
                    None
                }
            })
            .and_then(|t| to_iso_utc(Some(t)))
            .unwrap_or_else(|| ts.clone());

        let mut model_row = json!({
            "type": "message",
            "id": format!("{}_m", turn_id),
            "role": "model",
            "text": asst_text,
            "attachments": asst_attachments,
            "timestamp": model_ts,
            "model": model,
        });

        if let Some(thinking) = asst
            .and_then(|a| a.get("thinking"))
            .and_then(|v| v.as_str())
        {
            if !thinking.is_empty() {
                model_row["thinking"] = json!(thinking);
            }
        }
        if let Some(music_meta) = asst.and_then(|a| a.get("music_meta")) {
            if !music_meta.is_null() {
                model_row["musicMeta"] = music_meta.clone();
            }
        }
        if let Some(gen_meta) = asst.and_then(|a| a.get("gen_meta")) {
            if !gen_meta.is_null() {
                model_row["genMeta"] = gen_meta.clone();
            }
        }
        if let Some(deep_research) = asst.and_then(|a| a.get("deep_research")) {
            if !deep_research.is_null() {
                let mut dr = deep_research.clone();
                // 报告正文外置到 media 文件
                if dr.get("type").and_then(|v| v.as_str()) == Some("report") {
                    if let Some(text) = dr.get("report_text").and_then(|v| v.as_str()) {
                        if !text.is_empty() {
                            let media_id = new_media_id("md");
                            let size_bytes = text.as_bytes().len();
                            let char_count = text.chars().count();
                            let _ = std::fs::write(media_dir.join(&media_id), text.as_bytes());
                            dr.as_object_mut().map(|o| {
                                o.remove("report_text");
                                o.insert("report_media_id".to_string(), json!(media_id));
                                o.insert("size_bytes".to_string(), json!(size_bytes));
                                o.insert("char_count".to_string(), json!(char_count));
                            });
                        }
                    }
                    // 调研过程外置到 JSON media 文件，并注入统计字段
                    if let Some(entries) = dr.get("progress").and_then(|v| v.as_array()).cloned() {
                        if !entries.is_empty() {
                            let entry_count = entries.len();
                            let mut rounds: i64 = 0;
                            let mut thinking_count: usize = 0;
                            let mut web_count: usize = 0;
                            let mut file_count: usize = 0;
                            for e in &entries {
                                match e.get("type").and_then(|v| v.as_str()) {
                                    Some("thinking") => {
                                        thinking_count += 1;
                                        if let Some(r) = e.get("round").and_then(|v| v.as_i64()) {
                                            if r + 1 > rounds {
                                                rounds = r + 1;
                                            }
                                        }
                                    }
                                    Some("web_search") => web_count += 1,
                                    Some("file_search") => file_count += 1,
                                    _ => {}
                                }
                            }
                            let payload = Value::Array(entries);
                            let serialized =
                                serde_json::to_vec(&payload).unwrap_or_else(|_| b"[]".to_vec());
                            let media_id = new_media_id("json");
                            let size_bytes = serialized.len();
                            let _ = std::fs::write(media_dir.join(&media_id), &serialized);
                            dr.as_object_mut().map(|o| {
                                o.remove("progress");
                                o.insert("progress_media_id".to_string(), json!(media_id));
                                o.insert("progress_size_bytes".to_string(), json!(size_bytes));
                                o.insert("entry_count".to_string(), json!(entry_count));
                                o.insert("rounds".to_string(), json!(rounds));
                                o.insert("thinking_count".to_string(), json!(thinking_count));
                                o.insert("web_count".to_string(), json!(web_count));
                                o.insert("file_count".to_string(), json!(file_count));
                            });
                        }
                    }
                }
                model_row["deepResearch"] = dr;
            }
        }
        if let Some(canvas_arr) = asst
            .and_then(|a| a.get("canvas"))
            .and_then(|v| v.as_array())
        {
            if !canvas_arr.is_empty() {
                let mut externalized: Vec<Value> = Vec::new();
                for canvas in canvas_arr {
                    let mut cv = canvas.clone();
                    // Canvas 代码内容外置到 media 文件
                    if let Some(content) = cv.get("content").and_then(|v| v.as_str()) {
                        if !content.is_empty() {
                            let ext = cv
                                .get("filename")
                                .and_then(|v| v.as_str())
                                .and_then(|f| f.rsplit('.').next())
                                .unwrap_or("txt");
                            let media_id = new_media_id(ext);
                            let size_bytes = content.as_bytes().len();
                            let char_count = content.chars().count();
                            let _ = std::fs::write(media_dir.join(&media_id), content.as_bytes());
                            cv.as_object_mut().map(|o| {
                                o.remove("content");
                                o.insert("content_media_id".to_string(), json!(media_id));
                                o.insert("size_bytes".to_string(), json!(size_bytes));
                                o.insert("char_count".to_string(), json!(char_count));
                            });
                        }
                    }
                    externalized.push(cv);
                }
                model_row["canvas"] = json!(externalized);
            }
        }
        // content_blocks 直接透传（已由 turn_parser 生成）
        if let Some(blocks) = asst
            .and_then(|a| a.get("content_blocks"))
            .and_then(|v| v.as_array())
        {
            if !blocks.is_empty() {
                model_row["contentBlocks"] = json!(blocks);
            }
        }
        rows.push(model_row);
    }

    // 标记 action_card 消息为 hidden（仅处理 message 行，跳过第一行 meta）
    mark_action_card_hidden(&mut rows);

    rows
}