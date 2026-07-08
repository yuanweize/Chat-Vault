use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::storage;
use crate::str_err::ToStringErr;
use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::query::{BooleanQuery, BoostQuery, Occur, TermQuery};
use tantivy::schema::*;
use tantivy::tokenizer::{LowerCaser, NgramTokenizer, TextAnalyzer};
use tantivy::{doc, Index, IndexWriter, ReloadPolicy, TantivyDocument, Term};

const INDEX_DIR_NAME: &str = "search_index";
const MTIME_FILE: &str = "search_mtimes.json";
const WRITER_HEAP: usize = 50_000_000;

const F_CONV_ID: &str = "conversation_id";
const F_MSG_ID: &str = "message_id";
const F_ROLE: &str = "role";
const F_TITLE: &str = "title";
const F_TEXT: &str = "text";

fn build_schema() -> Schema {
    let mut builder = Schema::builder();

    let ngram_indexing = TextFieldIndexing::default()
        .set_tokenizer("ngram23")
        .set_index_option(IndexRecordOption::WithFreqsAndPositions);
    let ngram_stored = TextOptions::default()
        .set_indexing_options(ngram_indexing)
        .set_stored();

    builder.add_text_field(F_CONV_ID, STRING | STORED);
    builder.add_text_field(F_MSG_ID, STRING | STORED);
    builder.add_text_field(F_ROLE, STRING | STORED);
    builder.add_text_field(F_TITLE, ngram_stored.clone());
    builder.add_text_field(F_TEXT, ngram_stored);

    builder.build()
}

fn register_tokenizers(index: &Index) {
    let tokenizer = TextAnalyzer::builder(NgramTokenizer::new(2, 3, false).unwrap())
        .filter(LowerCaser)
        .build();
    index.tokenizers().register("ngram23", tokenizer);
}

/// 打开（或创建）指定账号的 tantivy 索引。
pub fn open_or_create_index(account_dir: &Path) -> Result<Index, String> {
    // 清理旧版 SQLite 搜索索引
    for suffix in ["", "-wal", "-shm"] {
        let p = account_dir.join(format!("search.db{}", suffix));
        if p.exists() {
            let _ = std::fs::remove_file(&p);
        }
    }

    let index_dir = account_dir.join(INDEX_DIR_NAME);
    std::fs::create_dir_all(&index_dir).map_err(|e| format!("创建索引目录失败: {}", e))?;

    let schema = build_schema();
    let dir = MmapDirectory::open(&index_dir).map_err(|e| format!("打开索引目录失败: {}", e))?;
    let index = Index::open_or_create(dir, schema).map_err(|e| format!("打开索引失败: {}", e))?;
    register_tokenizers(&index);

    Ok(index)
}

// ── mtime 辅助 ──────────────────────────────────────────────────────────

fn mtime_path(account_dir: &Path) -> std::path::PathBuf {
    account_dir.join(MTIME_FILE)
}

fn load_mtimes(account_dir: &Path) -> HashMap<String, f64> {
    let path = mtime_path(account_dir);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_mtimes(account_dir: &Path, mtimes: &HashMap<String, f64>) -> Result<(), String> {
    let json = serde_json::to_string(mtimes).str_err()?;
    std::fs::write(mtime_path(account_dir), json).map_err(|e| format!("保存 mtime 文件失败: {}", e))
}

fn file_mtime(path: &Path) -> f64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64()
        })
        .unwrap_or(0.0)
}

// ── action_card / hidden 过滤 ──────────────────────────────────────────

/// 检查消息行是否被标记为 hidden（由 storage::turns_to_jsonl_rows 在写入时标记）。
fn is_hidden(msg: &serde_json::Value) -> bool {
    msg.get("hidden").and_then(|v| v.as_bool()).unwrap_or(false)
}

// ── 索引写入 ────────────────────────────────────────────────────────────

fn index_jsonl_into_writer(
    writer: &mut IndexWriter,
    schema: &Schema,
    conv_id: &str,
    jsonl_path: &Path,
) -> Result<(), String> {
    let conv_id_field = schema.get_field(F_CONV_ID).unwrap();
    let msg_id_field = schema.get_field(F_MSG_ID).unwrap();
    let role_field = schema.get_field(F_ROLE).unwrap();
    let title_field = schema.get_field(F_TITLE).unwrap();
    let text_field = schema.get_field(F_TEXT).unwrap();

    let raw = std::fs::read_to_string(jsonl_path).str_err()?;
    let mut title = String::new();
    let mut messages: Vec<serde_json::Value> = Vec::new();

    for line in raw.lines() {
        let s = line.trim();
        if s.is_empty() {
            continue;
        }
        let row: serde_json::Value = match serde_json::from_str(s) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match row.get("type").and_then(|v| v.as_str()) {
            Some("meta") => {
                if let Some(t) = row.get("title").and_then(|v| v.as_str()) {
                    title = t.to_string();
                }
            }
            Some("message") => messages.push(row),
            _ => {}
        }
    }

    for msg in &messages {
        if is_hidden(msg) {
            continue;
        }
        let msg_id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let text = msg.get("text").and_then(|v| v.as_str()).unwrap_or("");
        if !text.is_empty() {
            writer
                .add_document(doc!(
                    conv_id_field => conv_id,
                    msg_id_field => msg_id,
                    role_field => role,
                    title_field => title.as_str(),
                    text_field => text,
                ))
                .str_err()?;
        }
    }

    Ok(())
}

/// 对单个对话文件进行增量索引（删旧插新）。
pub fn index_conversation(
    index: &Index,
    account_dir: &Path,
    conv_id: &str,
    jsonl_path: &Path,
) -> Result<(), String> {
    let mtime = file_mtime(jsonl_path);
    let mut mtimes = load_mtimes(account_dir);

    if let Some(&old) = mtimes.get(conv_id) {
        if (old - mtime).abs() < 0.001 {
            return Ok(());
        }
    }

    let schema = index.schema();
    let conv_id_field = schema.get_field(F_CONV_ID).unwrap();

    let mut writer: IndexWriter<TantivyDocument> = index
        .writer(WRITER_HEAP)
        .map_err(|e| format!("创建 writer 失败: {}", e))?;
    writer.delete_term(Term::from_field_text(conv_id_field, conv_id));
    index_jsonl_into_writer(&mut writer, &schema, conv_id, jsonl_path)?;
    writer
        .commit()
        .map_err(|e| format!("提交索引失败: {}", e))?;

    mtimes.insert(conv_id.to_string(), mtime);
    save_mtimes(account_dir, &mtimes)?;

    Ok(())
}

/// 从 conversations 目录全量增量索引（不做 segment merge）。
pub fn index_all(
    index: &Index,
    account_dir: &Path,
    conversations_dir: &Path,
) -> Result<u32, String> {
    if !conversations_dir.exists() {
        return Ok(0);
    }

    let t0 = std::time::Instant::now();
    log::info!("[index_all] start");

    let schema = index.schema();
    let conv_id_field = schema.get_field(F_CONV_ID).unwrap();

    let mut mtimes = load_mtimes(account_dir);
    let mut writer: IndexWriter<TantivyDocument> = index
        .writer(WRITER_HEAP)
        .map_err(|e| format!("创建 writer 失败: {}", e))?;

    let mut file_ids: HashSet<String> = HashSet::new();
    let entries = std::fs::read_dir(conversations_dir).str_err()?;
    let mut total = 0u32;
    let mut indexed = 0u32;
    let mut skipped = 0u32;

    for entry in entries.flatten() {
        let path = entry.path();
        if !storage::is_jsonl_file(&path) {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            let conv_id = stem.to_string();
            file_ids.insert(conv_id.clone());
            total += 1;

            let mtime = file_mtime(&path);
            if let Some(&old) = mtimes.get(&conv_id) {
                if (old - mtime).abs() < 0.001 {
                    skipped += 1;
                    continue;
                }
            }

            writer.delete_term(Term::from_field_text(conv_id_field, &conv_id));
            index_jsonl_into_writer(&mut writer, &schema, &conv_id, &path)?;
            mtimes.insert(conv_id, mtime);
            indexed += 1;
        }
    }

    // 清理已删除的对话
    let removed: Vec<String> = mtimes
        .keys()
        .filter(|id| !file_ids.contains(id.as_str()))
        .cloned()
        .collect();
    let deleted = removed.len();
    for id in &removed {
        writer.delete_term(Term::from_field_text(conv_id_field, id));
        mtimes.remove(id);
    }

    log::info!(
        "[index_all] scan done: total={}, indexed={}, skipped={}, deleted={}, elapsed={}ms",
        total,
        indexed,
        skipped,
        deleted,
        t0.elapsed().as_millis()
    );

    let t_commit = std::time::Instant::now();
    writer
        .commit()
        .map_err(|e| format!("提交索引失败: {}", e))?;
    log::info!(
        "[index_all] commit done: elapsed={}ms",
        t_commit.elapsed().as_millis()
    );

    save_mtimes(account_dir, &mtimes)?;

    log::info!(
        "[index_all] complete: total={}, indexed={}, total_elapsed={}ms",
        total,
        indexed,
        t0.elapsed().as_millis()
    );

    Ok(total)
}

/// 合并所有 segment，用于 rebuild 时清理碎片。
pub fn merge_segments(index: &Index) -> Result<(), String> {
    let seg_ids = index.searchable_segment_ids().unwrap_or_default();
    let seg_count = seg_ids.len();
    if seg_count <= 1 {
        log::info!("[merge_segments] skip: segments={}", seg_count);
        return Ok(());
    }
    log::info!("[merge_segments] start: segments={}", seg_count);
    let t0 = std::time::Instant::now();
    let mut writer: IndexWriter<TantivyDocument> = index
        .writer(WRITER_HEAP)
        .map_err(|e| format!("创建 writer 失败: {}", e))?;
    let _ = writer.merge(&seg_ids).wait();
    writer
        .commit()
        .map_err(|e| format!("合并提交失败: {}", e))?;
    let _ = writer.garbage_collect_files().wait();
    writer
        .wait_merging_threads()
        .map_err(|e| format!("等待合并失败: {}", e))?;
    log::info!(
        "[merge_segments] done: segments={}, elapsed={}ms",
        seg_count,
        t0.elapsed().as_millis()
    );
    Ok(())
}

/// 删除单个对话的索引。
pub fn remove_conversation(index: &Index, account_dir: &Path, conv_id: &str) -> Result<(), String> {
    let schema = index.schema();
    let conv_id_field = schema.get_field(F_CONV_ID).unwrap();

    let mut writer: IndexWriter<TantivyDocument> = index
        .writer(WRITER_HEAP)
        .map_err(|e| format!("创建 writer 失败: {}", e))?;
    writer.delete_term(Term::from_field_text(conv_id_field, conv_id));
    writer
        .commit()
        .map_err(|e| format!("提交索引失败: {}", e))?;

    let mut mtimes = load_mtimes(account_dir);
    mtimes.remove(conv_id);
    save_mtimes(account_dir, &mtimes)?;

    Ok(())
}

// ── 搜索 ────────────────────────────────────────────────────────────────

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub conversation_id: String,
    pub message_id: String,
    pub title: String,
    pub snippet: String,
    pub role: String,
    pub rank: f64,
}

/// 全文搜索。
pub fn search_messages(
    index: &Index,
    query_str: &str,
    limit: u32,
) -> Result<Vec<SearchResult>, String> {
    let query_str = query_str.trim();
    if query_str.is_empty() {
        return Ok(Vec::new());
    }

    let schema = index.schema();
    let conv_id_field = schema.get_field(F_CONV_ID).unwrap();
    let msg_id_field = schema.get_field(F_MSG_ID).unwrap();
    let role_field = schema.get_field(F_ROLE).unwrap();
    let title_field = schema.get_field(F_TITLE).unwrap();
    let text_field = schema.get_field(F_TEXT).unwrap();

    // 用 ngram23 tokenizer 对查询进行分词
    let tokenizer_manager = index.tokenizers();
    let mut tokenizer = tokenizer_manager
        .get("ngram23")
        .ok_or("找不到 ngram23 tokenizer")?;

    let mut tokens: Vec<String> = Vec::new();
    {
        let mut stream = tokenizer.token_stream(query_str);
        while stream.advance() {
            tokens.push(stream.token().text.clone());
        }
    }
    tokens.sort();
    tokens.dedup();

    if tokens.is_empty() {
        return Ok(Vec::new());
    }

    // 构建查询: AND( OR(text:token, title:token) ) for each token
    let must_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = tokens
        .iter()
        .map(|tok| {
            let text_term = Term::from_field_text(text_field, tok);
            let title_term = Term::from_field_text(title_field, tok);

            let text_q = TermQuery::new(text_term, IndexRecordOption::WithFreqsAndPositions);
            let title_q = TermQuery::new(title_term, IndexRecordOption::WithFreqsAndPositions);

            let or_query = BooleanQuery::new(vec![
                (
                    Occur::Should,
                    Box::new(BoostQuery::new(Box::new(text_q), 10.0))
                        as Box<dyn tantivy::query::Query>,
                ),
                (
                    Occur::Should,
                    Box::new(BoostQuery::new(Box::new(title_q), 1.0))
                        as Box<dyn tantivy::query::Query>,
                ),
            ]);
            (
                Occur::Must,
                Box::new(or_query) as Box<dyn tantivy::query::Query>,
            )
        })
        .collect();

    let final_query = BooleanQuery::new(must_clauses);

    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()
        .map_err(|e: tantivy::TantivyError| format!("创建 reader 失败: {}", e))?;
    let searcher = reader.searcher();

    let top_docs = searcher
        .search(&final_query, &TopDocs::with_limit(limit as usize))
        .map_err(|e| format!("搜索失败: {}", e))?;

    let query_lower = query_str.to_lowercase();
    let mut results = Vec::new();
    for (score, doc_addr) in top_docs {
        let doc: TantivyDocument = searcher.doc(doc_addr).str_err()?;
        let text = doc
            .get_first(text_field)
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let snippet = build_snippet(text, &query_lower);

        results.push(SearchResult {
            conversation_id: doc
                .get_first(conv_id_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            message_id: doc
                .get_first(msg_id_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            title: doc
                .get_first(title_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            snippet,
            role: doc
                .get_first(role_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            rank: score as f64,
        });
    }

    Ok(results)
}

/// HTML entity 转义，防止 XSS。
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
    out
}

/// 在 text 中定位 query 子串，截取前后上下文并用 <mark> 高亮。
fn build_snippet(text: &str, query_lower: &str) -> String {
    const CONTEXT_CHARS: usize = 40;

    let text_lower = text.to_lowercase();
    let pos = match text_lower.find(query_lower) {
        Some(p) => p,
        None => {
            // n-gram 匹配但原文子串不完全匹配（极少情况），返回开头
            let chars: String = text.chars().take(CONTEXT_CHARS * 2).collect();
            return chars;
        }
    };

    // 将字节偏移对齐到字符边界
    let match_end = pos + query_lower.len();

    // 向前取 CONTEXT_CHARS 个字符
    let start = {
        let mut count = 0;
        let mut idx = pos;
        while count < CONTEXT_CHARS && idx > 0 {
            idx = text[..idx]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            count += 1;
        }
        idx
    };

    // 向后取 CONTEXT_CHARS 个字符
    let end = {
        let mut count = 0;
        let mut idx = match_end;
        let bytes = text.as_bytes();
        while count < CONTEXT_CHARS && idx < bytes.len() {
            // 跳过 UTF-8 continuation bytes
            idx += 1;
            while idx < bytes.len() && (bytes[idx] & 0xC0) == 0x80 {
                idx += 1;
            }
            count += 1;
        }
        idx
    };

    let mut snippet = String::new();
    if start > 0 {
        snippet.push_str("...");
    }
    snippet.push_str(&html_escape(&text[start..pos]));
    snippet.push_str("<mark>");
    snippet.push_str(&html_escape(&text[pos..match_end]));
    snippet.push_str("</mark>");
    snippet.push_str(&html_escape(&text[match_end..end]));
    if end < text.len() {
        snippet.push_str("...");
    }
    snippet
}
