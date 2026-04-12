//! API 集成测试：真实 cookie + 真实 Gemini API
//!
//! 运行方式：
//!   TEST_ACCOUNT=xxx TEST_CONV_ID=c_xxx TEST_CONV_ID_2=c_yyy \
//!     cargo test --test api_integration -- --ignored --nocapture
//!
//! 环境变量：
//!   TEST_ACCOUNT    — 账号邮箱关键字（用于 email.contains() 匹配）
//!   TEST_CONV_ID    — 主测试对话 ID（含所有媒体类型）
//!   TEST_CONV_ID_2  — 第二个对话 ID（music_meta + gen_meta 交叉验证）
//!   TEST_DR_CONV_ID — Deep Research 对话 ID

use std::collections::HashSet;

use gemini_collector_lib::cookies;
use gemini_collector_lib::gemini_api::GeminiExporter;
use gemini_collector_lib::gemini_api::media_download::{DownloadStats, MediaDownloadItem};
use gemini_collector_lib::turn_parser::{parse_turn, normalize_turn_media_first_seen, ParsedTurn, MediaFile};

fn test_account() -> String {
    std::env::var("TEST_ACCOUNT").expect("需设置环境变量 TEST_ACCOUNT（账号邮箱关键字）")
}
fn test_conv_id() -> String {
    std::env::var("TEST_CONV_ID").expect("需设置环境变量 TEST_CONV_ID（主测试对话 ID）")
}
fn test_conv_id_2() -> String {
    std::env::var("TEST_CONV_ID_2").expect("需设置环境变量 TEST_CONV_ID_2（第二个对话 ID）")
}
fn test_dr_conv_id() -> String {
    std::env::var("TEST_DR_CONV_ID").expect("需设置环境变量 TEST_DR_CONV_ID（Deep Research 对话 ID）")
}
fn test_dr_conv_id_2() -> String {
    std::env::var("TEST_DR_CONV_ID_2").expect("需设置环境变量 TEST_DR_CONV_ID_2（进行中的 Deep Research 对话 ID）")
}

async fn init_exporter() -> GeminiExporter {
    let all_cookies = cookies::get_cookies_from_local_browser()
        .expect("无法从本机浏览器读取 cookies");
    let mappings = cookies::list_accounts::discover_email_authuser_mapping(&all_cookies)
        .await
        .expect("ListAccounts 失败");
    let account = test_account();
    let target = mappings.iter().find(|m| m.email.contains(&account))
        .expect(&format!("未找到 {} 账号", account));
    let mut exporter = GeminiExporter::new(all_cookies, target.authuser.clone(), None, None);
    exporter.init_auth().await.expect("init_auth 失败");
    exporter
}

/// 分析 Deep Research 对话的原始 turn 结构，dump 到指定目录
async fn dump_dr_conversation(exporter: &GeminiExporter, conv_id: &str, dump_dir: &std::path::Path) {
    let raw_turns = exporter.get_chat_detail(conv_id).await
        .expect("get_chat_detail 失败");
    eprintln!("\n[DR] 对话 {} 共 {} 轮", conv_id, raw_turns.len());

    let _ = std::fs::create_dir_all(dump_dir);

    for (i, turn) in raw_turns.iter().enumerate() {
        let ai_data = turn.as_array()
            .and_then(|a| a.get(3))
            .and_then(|d| d.as_array())
            .and_then(|d| d.first())
            .and_then(|c| c.as_array())
            .and_then(|c| c.first());

        let ai = match ai_data {
            Some(a) => a,
            None => { eprintln!("  turn[{}]: 无 ai_data", i); continue; }
        };

        // dump 整轮原始数据
        let turn_file = dump_dir.join(format!("turn_{}.json", i));
        let _ = std::fs::write(&turn_file, serde_json::to_string_pretty(turn).unwrap_or_default());

        // 检查 ai[12][8] 所有 key
        let meta_dict = ai.as_array()
            .and_then(|a| a.get(12))
            .and_then(|b| b.as_array())
            .and_then(|b| b.get(8));

        let keys: Vec<String> = meta_dict
            .and_then(|m| m.as_object())
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default();

        let has_56 = keys.contains(&"56".to_string());
        let has_57 = keys.contains(&"57".to_string());
        let has_58 = keys.contains(&"58".to_string());
        let has_70 = keys.contains(&"70".to_string());
        let has_30 = ai.as_array().and_then(|a| a.get(30)).map(|v| !v.is_null()).unwrap_or(false);

        // ai[30][0][10] 类型标记
        let block30_type = ai.as_array()
            .and_then(|a| a.get(30))
            .and_then(|b| b.as_array())
            .and_then(|a| a.first())
            .and_then(|item| item.as_array())
            .and_then(|a| a.get(10))
            .and_then(|v| v.as_i64());
        // ai[30][0][12] 完成标记
        let block30_done = ai.as_array()
            .and_then(|a| a.get(30))
            .and_then(|b| b.as_array())
            .and_then(|a| a.first())
            .and_then(|item| item.as_array())
            .and_then(|a| a.get(12))
            .and_then(|v| v.as_bool());

        let type_label = if has_30 { "HAS_30" } else if has_56 { "PLAN" } else { "NORMAL" };

        eprintln!("  turn[{}]: {} | keys={:?} 30={} 30_type={:?} 30_done={:?}",
            i, type_label, keys, has_30, block30_type, block30_done);

        // ai 数组长度
        if let Some(arr) = ai.as_array() {
            eprintln!("         ai len={}", arr.len());
        }

        // dump 每个 meta key
        if let Some(md) = meta_dict {
            for key in &keys {
                let kf = dump_dir.join(format!("turn_{}_{}.json", i, key));
                if let Some(v) = md.get(key.as_str()) {
                    let _ = std::fs::write(&kf, serde_json::to_string_pretty(v).unwrap_or_default());
                }
            }

            if let Some(data_58) = md.get("58") {
                let entry_count = data_58.as_array()
                    .and_then(|a| a.get(1))
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.get(4))
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.get(2))
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                let data_len = serde_json::to_string(data_58).map(|s| s.len()).unwrap_or(0);
                eprintln!("         58 条目数={}, JSON 字节={}", entry_count, data_len);
            }
        }

        // 解析检查
        let parsed = parse_turn(turn);
        if let Some(ref dr) = parsed.assistant.deep_research {
            let (dr_type, progress_len) = match dr {
                gemini_collector_lib::turn_parser::DeepResearch::Plan { .. } =>
                    ("plan", 0),
                gemini_collector_lib::turn_parser::DeepResearch::Report { progress, .. } =>
                    ("report", progress.as_ref().map(|p| p.len()).unwrap_or(0)),
            };
            eprintln!("         parsed: type={}, progress_entries={}", dr_type, progress_len);
        } else {
            eprintln!("         parsed: no deep_research");
        }

        // assistant text 片段
        let atxt = &parsed.assistant.text;
        if !atxt.is_empty() {
            let preview: String = atxt.chars().take(100).collect();
            eprintln!("         text: {}...", preview);
        }
    }

    eprintln!("\n[DR] 原始数据已 dump 到 {}", dump_dir.display());
}

// ============================================================================
// 辅助函数
// ============================================================================

fn find_user_media<'a>(turns: &'a [ParsedTurn], media_type: &str) -> Vec<&'a MediaFile> {
    turns.iter().flat_map(|t| &t.user.files).filter(|f| f.media_type == media_type).collect()
}

fn find_assistant_media<'a>(turns: &'a [ParsedTurn], media_type: &str) -> Vec<&'a MediaFile> {
    turns.iter().flat_map(|t| &t.assistant.files).filter(|f| f.media_type == media_type).collect()
}

fn find_music_turn(turns: &[ParsedTurn]) -> Option<&ParsedTurn> {
    turns.iter().find(|t| t.assistant.music_meta.is_some())
}

fn find_gen_meta_turn(turns: &[ParsedTurn]) -> Option<&ParsedTurn> {
    turns.iter().find(|t| t.assistant.gen_meta.is_some())
}

async fn assert_media_downloadable(exporter: &GeminiExporter, file: &MediaFile, label: &str) {
    let url = file.url.as_ref().expect(&format!("[{}] url 不应为空", label));
    assert!(url.starts_with("https://"), "[{}] URL 应以 https:// 开头", label);

    let tmp_dir = tempfile::tempdir().expect("创建临时目录失败");
    let ext = match file.media_type.as_str() {
        "image" => "jpg", "video" => "mp4", "audio" => "mp3", "attachment" => "bin", _ => "bin",
    };
    let filepath = tmp_dir.path().join(format!("test.{}", ext));

    let item = MediaDownloadItem {
        url: url.clone(),
        filepath: filepath.clone(),
        media_id: format!("test.{}", ext),
        media_type: Some(file.media_type.clone()),
    };

    let mut stats = DownloadStats::default();
    let failed = exporter.download_media_batch(&[item], &mut stats).await;

    assert!(failed.is_empty(), "[{}] 下载失败: {:?}", label, failed);
    assert!(filepath.exists(), "[{}] 文件不存在", label);
    let size = std::fs::metadata(&filepath).unwrap().len();
    assert!(size > 0, "[{}] 文件大小为 0", label);
    eprintln!("    download ok: {} bytes", size);
    // tmp_dir drop 时自动清理
}

// ============================================================================
// 主测试
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_api_full_pipeline() {
    let conv_id = test_conv_id();
    let conv_id_2 = test_conv_id_2();
    let mut passed = 0u32;
    let mut known_issues = 0u32;

    // ================================================================
    // Phase 1: 认证
    // ================================================================
    eprintln!("\n[Phase 1] 认证 ─────────────────────────────────");
    let all_cookies = cookies::get_cookies_from_local_browser()
        .expect("无法从本机浏览器读取 cookies");

    let mappings = cookies::list_accounts::discover_email_authuser_mapping(&all_cookies)
        .await
        .expect("ListAccounts 失败");
    let account = test_account();
    let target = mappings
        .iter()
        .find(|m| m.email.contains(&account))
        .expect(&format!("未找到 {} 账号", account));
    eprintln!("  账号: {} authuser={:?}", target.email, target.authuser);

    let mut exporter = GeminiExporter::new(all_cookies, target.authuser.clone(), None, None);
    exporter.init_auth().await.expect("init_auth 失败");

    assert!(exporter.at.is_some(), "at 应非空");
    assert!(exporter.bl.is_some(), "bl 应非空");
    assert!(exporter.fsid.is_some(), "fsid 应非空");
    eprintln!("  [PASS] at/bl/fsid 均已获取");
    passed += 1;

    // ================================================================
    // Phase 2: 聊天列表分页
    // ================================================================
    eprintln!("\n[Phase 2] 聊天列表 ─────────────────────────────");
    let (chats, next_token) = exporter
        .get_chats_page(None)
        .await
        .expect("get_chats_page 失败");

    assert!(!chats.is_empty(), "聊天列表不应为空");
    for chat in &chats {
        assert!(!chat.id.is_empty(), "chat.id 不应为空");
    }
    eprintln!(
        "  [PASS] {} 个对话, has_next={}",
        chats.len(),
        next_token.is_some()
    );
    passed += 1;

    // ================================================================
    // Phase 3: 对话详情
    // ================================================================
    eprintln!("\n[Phase 3] 对话详情 ─────────────────────────────");
    let raw_turns = exporter
        .get_chat_detail(&conv_id)
        .await
        .expect("get_chat_detail 失败");

    assert!(!raw_turns.is_empty(), "对话应有 turns");
    let mut parsed_turns: Vec<ParsedTurn> = raw_turns.iter().map(|t| parse_turn(t)).collect();
    normalize_turn_media_first_seen(&mut parsed_turns);

    for (i, turn) in parsed_turns.iter().enumerate() {
        assert!(turn.turn_id.is_some(), "turn[{}] 应有 turn_id", i);
    }
    eprintln!("  [PASS] {} turns", parsed_turns.len());
    passed += 1;

    // ================================================================
    // Phase 4: 增量抓取
    // ================================================================
    eprintln!("\n[Phase 4] 增量抓取 ─────────────────────────────");
    let all_ids: Vec<String> = parsed_turns
        .iter()
        .filter_map(|t| t.turn_id.clone())
        .collect();
    if all_ids.len() >= 2 {
        let half = all_ids.len() / 2;
        let existing: HashSet<String> = all_ids[..half].iter().cloned().collect();
        let new_turns = exporter
            .get_chat_detail_incremental(&conv_id, &existing)
            .await
            .expect("incremental 失败");

        for raw in &new_turns {
            let p = parse_turn(raw);
            if let Some(ref tid) = p.turn_id {
                assert!(!existing.contains(tid), "增量不应含已有 turn: {}", tid);
            }
        }
        eprintln!("  [PASS] existing={}, new={}", existing.len(), new_turns.len());
        passed += 1;
    }

    // ================================================================
    // Phase 5: 媒体覆盖汇总
    // ================================================================
    eprintln!("\n[Phase 5] 媒体覆盖汇总 ─────────────────────────");
    for (i, turn) in parsed_turns.iter().enumerate() {
        let tid = turn.turn_id.as_deref().unwrap_or("?");
        let txt = if turn.user.text.len() > 50 {
            format!("{}...", &turn.user.text[..50])
        } else {
            turn.user.text.clone()
        };
        eprintln!("  turn[{}] {} {:?}", i, tid, txt);
        for f in &turn.user.files {
            eprintln!("    [user]      type={:<12} file={:?}", f.media_type, f.filename);
        }
        for f in &turn.assistant.files {
            eprintln!("    [assistant] type={:<12} file={:?}", f.media_type, f.filename);
        }
        if turn.assistant.music_meta.is_some() {
            eprintln!("    [assistant] music_meta ✓");
        }
        if turn.assistant.gen_meta.is_some() {
            eprintln!("    [assistant] gen_meta ✓");
        }
    }

    let user_images = find_user_media(&parsed_turns, "image");
    let user_videos = find_user_media(&parsed_turns, "video");
    let user_audios = find_user_media(&parsed_turns, "audio");
    let user_attachments = find_user_media(&parsed_turns, "attachment");
    let ai_images = find_assistant_media(&parsed_turns, "image");
    let ai_videos = find_assistant_media(&parsed_turns, "video");

    eprintln!();
    eprintln!("  用户图片={} 视频={} 音频={} 附件={}",
        user_images.len(), user_videos.len(), user_audios.len(), user_attachments.len());
    eprintln!("  AI 图片={} 视频={}", ai_images.len(), ai_videos.len());

    // ================================================================
    // Phase 6: 用户上传图片
    // ================================================================
    eprintln!("\n[Phase 6] 用户上传图片 ─────────────────────────");
    assert!(!user_images.is_empty(), "缺少用户上传图片");
    let img = user_images[0];
    eprintln!("  filename={:?} mime={:?}", img.filename, img.mime);
    assert_media_downloadable(&exporter, img, "user_image").await;
    eprintln!("  [PASS]");
    passed += 1;

    // ================================================================
    // Phase 7: 用户上传视频
    // ================================================================
    eprintln!("\n[Phase 7] 用户上传视频 ─────────────────────────");
    assert!(!user_videos.is_empty(), "缺少用户上传视频");
    let vid = user_videos[0];
    eprintln!("  filename={:?} mime={:?} duration={:?}", vid.filename, vid.mime, vid.duration);
    assert_media_downloadable(&exporter, vid, "user_video").await;
    eprintln!("  [PASS]");
    passed += 1;

    // ================================================================
    // Phase 8: 用户上传附件
    // ================================================================
    eprintln!("\n[Phase 8] 用户上传附件 ─────────────────────────");
    assert!(!user_attachments.is_empty(), "缺少用户上传附件");
    let att = user_attachments[0];
    assert!(att.filename.is_some(), "attachment 应有 filename");
    eprintln!("  filename={:?} mime={:?}", att.filename, att.mime);
    assert_media_downloadable(&exporter, att, "user_attachment").await;
    eprintln!("  [PASS]");
    passed += 1;

    // ================================================================
    // Phase 9: 用户上传音频
    // ================================================================
    eprintln!("\n[Phase 9] 用户上传音频 ─────────────────────────");
    assert!(!user_audios.is_empty(), "缺少用户上传音频");
    let aud = user_audios[0];
    eprintln!("  filename={:?} mime={:?}", aud.filename, aud.mime);
    assert_media_downloadable(&exporter, aud, "user_audio").await;
    eprintln!("  [PASS]");
    passed += 1;

    // ================================================================
    // Phase 10: AI 生成图片
    // ================================================================
    eprintln!("\n[Phase 10] AI 生成图片 ─────────────────────────");
    assert!(!ai_images.is_empty(), "缺少 AI 生成图片");
    let ai_img = ai_images[0];
    eprintln!("  filename={:?} mime={:?} resolution={:?}", ai_img.filename, ai_img.mime, ai_img.resolution);
    assert_media_downloadable(&exporter, ai_img, "ai_image").await;
    eprintln!("  [PASS]");
    passed += 1;

    // ================================================================
    // Phase 11: AI 生成图片 gen_meta
    // ================================================================
    eprintln!("\n[Phase 11] AI gen_meta (图片/视频) ─────────────");
    let gen_meta_turn = find_gen_meta_turn(&parsed_turns);
    assert!(gen_meta_turn.is_some(), "应有 gen_meta");
    let gm = gen_meta_turn.unwrap().assistant.gen_meta.as_ref().unwrap();
    eprintln!("  model={:?} prompt={:?}", gm.model, gm.prompt.as_ref().map(|p| &p[..p.len().min(80)]));
    // model 应已清理：不含 "models/" 前缀和 ";" 后缀
    if let Some(ref model) = gm.model {
        assert!(!model.starts_with("models/"), "model 不应含 models/ 前缀: {}", model);
        assert!(!model.contains(';'), "model 不应含 ';': {}", model);
    }
    eprintln!("  [PASS]");
    passed += 1;

    // ================================================================
    // Phase 12: AI 生成视频
    // ================================================================
    eprintln!("\n[Phase 12] AI 生成视频 ─────────────────────────");
    assert!(!ai_videos.is_empty(), "缺少 AI 生成视频");
    let ai_vid = ai_videos[0];
    eprintln!("  filename={:?} mime={:?}", ai_vid.filename, ai_vid.mime);
    assert_media_downloadable(&exporter, ai_vid, "ai_video").await;
    eprintln!("  [PASS]");
    passed += 1;

    // ================================================================
    // Phase 13: AI 生成音乐（文件下载 + music_meta）
    // ================================================================
    eprintln!("\n[Phase 13] AI 生成音乐 ─────────────────────────");
    // 找到音乐 turn（最后一个有 AI 音频的 turn）
    let music_turn_idx = parsed_turns.iter().rposition(|t|
        t.assistant.files.iter().any(|f| f.media_type == "audio")
    );
    assert!(music_turn_idx.is_some(), "缺少 AI 生成音乐 turn");
    let music_turn = &parsed_turns[music_turn_idx.unwrap()];
    let music_audio = music_turn.assistant.files.iter()
        .find(|f| f.media_type == "audio")
        .expect("音乐 turn 应有 audio 文件");
    eprintln!("  filename={:?} mime={:?}", music_audio.filename, music_audio.mime);
    assert_media_downloadable(&exporter, music_audio, "ai_music").await;
    eprintln!("  [PASS] 音频下载成功");
    passed += 1;

    // music_meta 检测（应已修复 Object 格式解析）
    let music_meta_turn = find_music_turn(&parsed_turns);
    assert!(music_meta_turn.is_some(), "music_meta 应已解析");
    let mm = music_meta_turn.unwrap().assistant.music_meta.as_ref().unwrap();
    eprintln!("  music_meta: title={:?} album={:?} genre={:?} moods={:?} caption={:?}",
        mm.title, mm.album, mm.genre, mm.moods,
        mm.caption.as_ref().map(|c| &c[..c.len().min(80)]));
    eprintln!("  [PASS] music_meta 已解析");
    passed += 1;

    // ================================================================
    // Phase 14: 异常检测
    // ================================================================
    eprintln!("\n[Phase 14] 异常检测 ─────────────────────────────");

    // type=unknown 文件
    let unknowns: Vec<(usize, &MediaFile)> = parsed_turns.iter().enumerate()
        .flat_map(|(i, t)| {
            t.user.files.iter().chain(t.assistant.files.iter())
                .filter(|f| f.media_type == "unknown")
                .map(move |f| (i, f))
        })
        .collect();
    if !unknowns.is_empty() {
        eprintln!("  [KNOWN ISSUE] {} 个 type=unknown:", unknowns.len());
        for (i, f) in &unknowns {
            eprintln!("    turn[{}]: {:?} mime={:?} url={}", i, f.filename, f.mime, f.url.is_some());
        }
        known_issues += 1;
    }

    // normalize 后不应再有 assistant 重复 attachment
    let asst_att_filenames: Vec<Option<&str>> = parsed_turns.iter()
        .flat_map(|t| t.assistant.files.iter())
        .filter(|f| f.media_type == "attachment")
        .map(|f| f.filename.as_deref())
        .collect();
    if asst_att_filenames.len() > 1 {
        let first = asst_att_filenames[0];
        let all_same = asst_att_filenames.iter().all(|n| *n == first);
        if all_same {
            eprintln!(
                "  [WARN] normalize 后仍有 {} 个相同 assistant attachment: {:?}",
                asst_att_filenames.len(), first
            );
        }
    } else {
        eprintln!("  [PASS] assistant attachment 去重正常");
    }

    eprintln!("  [DONE]");

    // ================================================================
    // 最终汇总
    // ================================================================
    eprintln!("\n══════════════════════════════════════════════════");
    eprintln!("  PASSED: {}, KNOWN ISSUES: {}", passed, known_issues);
    if known_issues > 0 {
        eprintln!("  已知问题需后续修复，不阻塞测试通过");
    }
    eprintln!("══════════════════════════════════════════════════\n");

    // ================================================================
    // Phase 15: 第二个对话 music_meta 验证
    // ================================================================
    eprintln!("[Phase 15] 对话 {} music_meta ──", conv_id_2);
    let raw2 = exporter
        .get_chat_detail(&conv_id_2)
        .await
        .expect("get_chat_detail(conv2) 失败");
    let mut parsed2: Vec<ParsedTurn> = raw2.iter().map(|t| parse_turn(t)).collect();
    normalize_turn_media_first_seen(&mut parsed2);

    eprintln!("  {} turns", parsed2.len());
    for (i, t) in parsed2.iter().enumerate() {
        let txt = if t.user.text.len() > 50 { format!("{}...", &t.user.text[..50]) } else { t.user.text.clone() };
        eprintln!("  turn[{}] {:?}", i, txt);
        for f in &t.assistant.files {
            eprintln!("    [assistant] type={:<12} file={:?}", f.media_type, f.filename);
        }
        if let Some(ref mm) = t.assistant.music_meta {
            eprintln!("    music_meta: title={:?} album={:?} genre={:?} moods={:?}",
                mm.title, mm.album, mm.genre, mm.moods);
            eprintln!("    caption: {:?}", mm.caption.as_ref().map(|c| &c[..c.len().min(100)]));
        }
        if let Some(ref gm) = t.assistant.gen_meta {
            eprintln!("    gen_meta: model={:?} prompt={:?}",
                gm.model, gm.prompt.as_ref().map(|p| &p[..p.len().min(80)]));
        }
    }

    let music2 = find_music_turn(&parsed2);
    assert!(music2.is_some(), "{} 应有 music_meta", conv_id_2);
    let mm2 = music2.unwrap().assistant.music_meta.as_ref().unwrap();
    assert!(mm2.title.is_some(), "music_meta.title 应非空");
    eprintln!("  [PASS] music_meta: title={:?} genre={:?}", mm2.title, mm2.genre);
    passed += 1;

    // gen_meta model 格式验证（第二个对话）
    let gen2 = find_gen_meta_turn(&parsed2);
    if let Some(t) = gen2 {
        let gm2 = t.assistant.gen_meta.as_ref().unwrap();
        if let Some(ref model) = gm2.model {
            assert!(!model.starts_with("models/"), "conv2 model 不应含 models/ 前缀: {}", model);
            assert!(!model.contains(';'), "conv2 model 不应含 ';': {}", model);
            eprintln!("  [PASS] gen_meta model={:?}", model);
        }
        passed += 1;
    }

    eprintln!("\n══════════════════════════════════════════════════");
    eprintln!("  FINAL: PASSED={}, KNOWN ISSUES={}", passed, known_issues);
    eprintln!("══════════════════════════════════════════════════\n");
}

/// Deep Research 已完成对话：验证 plan/report turn 结构
#[tokio::test]
#[ignore]
async fn test_deep_research_completed() {
    let exporter = init_exporter().await;
    let dump_dir = std::path::Path::new("/tmp/gemini_dr_dump/completed");
    dump_dr_conversation(&exporter, &test_dr_conv_id(), dump_dir).await;
}

/// Deep Research 进行中对话：分析中间状态的 turn 结构
#[tokio::test]
#[ignore]
async fn test_deep_research_in_progress() {
    let exporter = init_exporter().await;
    let dump_dir = std::path::Path::new("/tmp/gemini_dr_dump/in_progress");
    dump_dr_conversation(&exporter, &test_dr_conv_id_2(), dump_dir).await;
}

