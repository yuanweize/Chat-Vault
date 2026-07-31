pub fn parse_turn(turn: &Value) -> ParsedTurn {
    let mut result = ParsedTurn {
        turn_id: None,
        timestamp: None,
        timestamp_iso: None,
        user: RoleContent {
            text: String::new(),
            files: Vec::new(),
        },
        assistant: AssistantContent {
            text: String::new(),
            thinking: String::new(),
            model: String::new(),
            files: Vec::new(),
            music_meta: None,
            gen_meta: None,
            deep_research: None,
            canvas: Vec::new(),
            content_blocks: Vec::new(),
        },
    };

    let arr = match turn.as_array() {
        Some(a) => a,
        None => return result,
    };

    // turn_id from turn[0]
    if let Some(ids) = arr.first().and_then(|v| v.as_array()) {
        result.turn_id = if ids.len() > 1 {
            ids[1].as_str().map(|s| s.to_string())
        } else {
            ids.first().and_then(|v| v.as_str()).map(|s| s.to_string())
        };
    }

    // timestamp from turn[4]
    if arr.len() > 4 {
        if let Some(t4) = arr[4].as_array() {
            if !t4.is_empty() {
                if let Some(ts) = t4[0].as_i64() {
                    result.timestamp = Some(ts);
                    result.timestamp_iso = to_iso_utc(Some(ts));
                }
            }
        }
    }

    // user text from turn[2][0][0]
    if arr.len() > 2 {
        let content = &arr[2];
        if let Some(msg) = vget(content, 0) {
            if let Some(text) = vstr(msg, 0) {
                result.user.text = text.to_string();
            }
            // user files from msg[4][0][3]
            if let Some(m4) = vget(msg, 4) {
                if let Some(m40) = vget(m4, 0) {
                    if let Some(user_files) = varr(m40, 3) {
                        for f in user_files {
                            if f.is_array() {
                                result.user.files.push(parse_media_item(f, "user"));
                            }
                        }
                    }
                }
            }
        }
    }

    // assistant data from turn[3]
    if arr.len() <= 3 {
        return result;
    }
    let detail = &arr[3];
    let detail_arr = match detail.as_array() {
        Some(a) => a,
        None => return result,
    };

    // model from detail[21]
    if let Some(model) = detail_arr.get(21).and_then(|v| v.as_str()) {
        result.assistant.model = model.to_string();
    }

    // select AI candidate
    let mut ai_data: Option<&Value> = None;
    let selected_candidate_id = detail_arr.get(3).and_then(|v| v.as_str());

    if let Some(candidates_arr) = detail_arr.first().and_then(|v| v.as_array()) {
        let candidates: Vec<&Value> = candidates_arr.iter().filter(|c| c.is_array()).collect();

        if let Some(sel_id) = selected_candidate_id {
            for c in &candidates {
                if vstr(c, 0) == Some(sel_id) {
                    ai_data = Some(c);
                    break;
                }
            }
        }
        if ai_data.is_none() && !candidates.is_empty() {
            ai_data = Some(candidates[0]);
        }
    }

    // Build user media keys for dedup
    let user_media_keys: HashSet<(String, String, String, String)> = result
        .user
        .files
        .iter()
        .map(|f| {
            (
                f.url.clone().unwrap_or_default(),
                f.filename.clone().unwrap_or_default(),
                f.mime.clone().unwrap_or_default(),
                f.media_type.clone(),
            )
        })
        .collect();

    if let Some(ai) = ai_data {
        // assistant text from ai_data[1][0]
        if let Some(text_arr) = vget(ai, 1) {
            if let Some(text) = vstr(text_arr, 0) {
                result.assistant.text = text.to_string();
            }
        }

        // thinking from ai_data[37]
        if vlen(ai) > 37 && !ai[37].is_null() {
            if let Some(thinking_arr) = ai[37].as_array() {
                if !thinking_arr.is_empty() {
                    if let Some(inner) = thinking_arr[0].as_array() {
                        if !inner.is_empty() {
                            if let Some(s) = inner[0].as_str() {
                                result.assistant.thinking = s.to_string();
                            }
                        }
                    } else if let Some(s) = thinking_arr[0].as_str() {
                        result.assistant.thinking = s.to_string();
                    }
                }
            }
        }

        // AI media items
        let ai_media_items = extract_ai_media_items(ai);
        let mut seen_ai: HashSet<(String, String, String, String)> = HashSet::new();
        for item in &ai_media_items {
            let parsed = parse_media_item(item, "assistant");
            let key = (
                parsed.url.clone().unwrap_or_default(),
                parsed.filename.clone().unwrap_or_default(),
                parsed.mime.clone().unwrap_or_default(),
                parsed.media_type.clone(),
            );
            if user_media_keys.contains(&key) || seen_ai.contains(&key) {
                continue;
            }
            seen_ai.insert(key);
            result.assistant.files.push(parsed);
        }

        // Deep Research: report turn 有 ai[30]，plan turn 有 ai[12][8]["56"]
        // 两者都可能附带 ai[12][8]["58"] 进度数据（已在各自提取函数内处理）
        result.assistant.deep_research =
            extract_deep_research_report(ai).or_else(|| extract_deep_research_plan(ai));

        // Report turn 的 turn[4][0] 是 turn 创建时间，与 plan turn 几乎一致；
        // 真正的报告完成时间在 ai[12][8]["57"][0][5]，注入到 Report 里，后续仅用于
        // 覆盖 assistant 消息行的时间，user 消息行仍保留 turn[4][0]。
        if let Some(DeepResearch::Report { completion_ts, .. }) =
            result.assistant.deep_research.as_mut()
        {
            *completion_ts = extract_deep_research_completion_ts(ai);
        }

        // Canvas
        result.assistant.canvas = extract_canvas_list(ai);

        // Sanitize placeholder text (deep_research / canvas turn 的正文也是占位 URL)
        let needs_sanitize = !result.assistant.files.is_empty()
            || result.assistant.deep_research.is_some()
            || !result.assistant.canvas.is_empty();
        result.assistant.text = sanitize_generation_placeholder_text(
            &result.assistant.text,
            needs_sanitize,
            result.assistant.canvas.len(),
        );

        // Canvas 交错内容块
        if !result.assistant.canvas.is_empty() {
            result.assistant.content_blocks =
                split_text_into_content_blocks(&result.assistant.text);
        }

        // Strip citation markers ([cite_start], [cite: N, ...])
        result.assistant.text = strip_citation_markers(&result.assistant.text);

        // Generated media (music/video from ai_data[12])
        let (gen_files, music_meta, gen_meta) = extract_generated_media(ai);
        if !gen_files.is_empty() {
            let existing_urls: HashSet<String> = result
                .assistant
                .files
                .iter()
                .filter_map(|f| f.url.clone())
                .collect();
            for gf in gen_files {
                if let Some(ref u) = gf.url {
                    if !existing_urls.contains(u) {
                        result.assistant.files.push(gf);
                    }
                } else {
                    result.assistant.files.push(gf);
                }
            }
        }
        if music_meta.is_some() {
            result.assistant.music_meta = music_meta;
        }
        if gen_meta.is_some() {
            result.assistant.gen_meta = gen_meta;
        }
    }

    result
}