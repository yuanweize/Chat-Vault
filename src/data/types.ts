// Type interfaces used by frontend state and Tauri payloads.

export interface Attachment {
  mediaId: string;   // filename with extension in media/ dir
  mimeType: string;
  size?: number;     // file size in bytes, injected by Rust at load time
  previewMediaId?: string; // optional preview image for video
  downloadFailed?: boolean;
  downloadError?: string;
}

export interface ConvMessage {
  type: "message";
  id: string;
  role: "user" | "model";
  text: string;
  attachments: Attachment[];
  timestamp: string;   // ISO 8601
  model?: string;      // only when role=="model"
  thinking?: string;   // only when role=="model" and thinking exists
  genMeta?: { model?: string; prompt?: string };  // AI generated media meta
  hidden?: boolean;    // action_card messages marked by backend
  deepResearch?: {
    type: "plan" | "report";
    title: string;
    steps?: { number: number; name: string; description?: string }[];
    estimated_time?: string;
    report_media_id?: string;  // media file id, load via read_media_file
    research_id?: string;
    document_id?: string;
    size_bytes?: number;   // report media file byte size
    char_count?: number;   // report text char count
    // 调研过程：数组外置为 JSON media 文件，JSONL 中只留摘要统计
    progress_media_id?: string;
    progress_size_bytes?: number;
    entry_count?: number;
    rounds?: number;
    thinking_count?: number;
    web_count?: number;
    file_count?: number;
  };
  canvas?: {
    title: string;
    filename: string;
    content_media_id?: string;  // media file id, load via read_media_file
    language: string;
    document_id?: string;
    size_bytes?: number;   // canvas media file byte size
    char_count?: number;   // canvas content char count
  }[];
  contentBlocks?: (
    | { kind: "text"; text: string }
    | { kind: "canvas"; canvas_index: number }
  )[];
}

export interface Conversation {
  id: string;
  accountId: string;
  title: string;
  createdAt: string;   // ISO 8601
  updatedAt: string;   // ISO 8601
  remoteHash: string | null;
  parseWarning?: string;
  messages: ConvMessage[];
}

export interface ConversationSummary {
  id: string;
  title: string;
  lastMessage: string;      // plain text, max 80 chars
  messageCount: number;
  hasMedia: boolean;
  hasFailedData?: boolean;
  imageCount?: number;
  videoCount?: number;
  status?: string;          // normal | lost | hidden | ...
  updatedAt: string;        // ISO 8601
  createdAt?: string;       // ISO 8601, injected from .jsonl meta at load time
  remoteHash: string | null;
}

export interface SearchResult {
  conversationId: string;
  messageId: string;
  title: string;
  snippet: string;
  role: string;
  rank: number;
}

export interface Account {
  id: string;
  name: string;
  email: string;
  avatarText: string;
  avatarColor: string;
  conversationCount: number;
  remoteConversationCount: number | null;
  lastSyncAt: string | null;  // ISO 8601
  lastSyncResult: "success" | "partial" | "failed" | null;
  authuser?: string | null;
  listSyncPending?: boolean;
}
