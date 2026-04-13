import React, { useMemo, useState, useRef, useEffect } from "react";
import ReactDOM from "react-dom";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import rehypeRaw from "rehype-raw";
import "katex/dist/katex.min.css";
import { Virtuoso, VirtuosoHandle } from "react-virtuoso";
import { convertFileSrc } from "@tauri-apps/api/core";
import { openUrl, openPath } from "@tauri-apps/plugin-opener";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneLight, vscDarkPlus } from "react-syntax-highlighter/dist/esm/styles/prism";
import { Attachment, Conversation, ConvMessage } from "../data/types";
import { useTheme } from "../theme";
import { CopyIcon, CheckIcon, ChevronRightIcon, DocIcon, SparkIcon, SearchIcon, ExternalLinkIcon } from "./Icons";
import { ResearchDetailModal, ResearchModalState } from "./ResearchDetailModal";

const loadedImageUrlCache = new Set<string>();

function getKind(mimeType: string): "image" | "video" | "audio" | "file" {
  if (mimeType.startsWith("image/")) return "image";
  if (mimeType.startsWith("video/")) return "video";
  if (mimeType.startsWith("audio/")) return "audio";
  return "file";
}

function buildUrl(mediaId: string, mediaDir?: string, cacheKey?: string): string {
  if (!mediaDir || !mediaId) return "";
  const base = convertFileSrc(`${mediaDir}/${mediaId}`);
  if (!cacheKey) return base;
  return `${base}?v=${encodeURIComponent(cacheKey)}`;
}

function dedupeLikelyFormatVariants(attachments: Attachment[]): Attachment[] {
  // Gemini image_generation 在部分版本会返回同一图片的 png/jpeg 双格式；优先保留 png。
  if (attachments.length !== 2) return attachments;

  const imageAttachments = attachments.filter((a) => getKind(a.mimeType) === "image");
  if (imageAttachments.length !== 2) return attachments;

  const mimes = imageAttachments.map((a) => (a.mimeType || "").toLowerCase());
  const hasPng = mimes.includes("image/png");
  const hasJpeg = mimes.includes("image/jpeg") || mimes.includes("image/jpg");
  if (!hasPng || !hasJpeg) return attachments;

  const preferred = imageAttachments.find((a) => (a.mimeType || "").toLowerCase() === "image/png") ?? imageAttachments[0];
  return [preferred];
}

function hammingDistance(a: number[], b: number[]): number {
  if (a.length !== b.length) return Number.MAX_SAFE_INTEGER;
  let diff = 0;
  for (let i = 0; i < a.length; i += 1) {
    if (a[i] !== b[i]) diff += 1;
  }
  return diff;
}

async function computeImageDHash(url: string, size = 8): Promise<number[] | null> {
  return new Promise((resolve) => {
    const img = new Image();
    img.decoding = "async";
    img.onload = () => {
      try {
        const canvas = document.createElement("canvas");
        canvas.width = size + 1;
        canvas.height = size;
        const ctx = canvas.getContext("2d");
        if (!ctx) {
          resolve(null);
          return;
        }
        ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
        const { data } = ctx.getImageData(0, 0, canvas.width, canvas.height);
        const gray: number[] = [];
        for (let i = 0; i < data.length; i += 4) {
          gray.push((data[i] * 0.299) + (data[i + 1] * 0.587) + (data[i + 2] * 0.114));
        }
        const bits: number[] = [];
        for (let y = 0; y < size; y += 1) {
          const rowOffset = y * (size + 1);
          for (let x = 0; x < size; x += 1) {
            bits.push(gray[rowOffset + x] > gray[rowOffset + x + 1] ? 1 : 0);
          }
        }
        resolve(bits);
      } catch {
        resolve(null);
      }
    };
    img.onerror = () => resolve(null);
    img.src = url;
  });
}

// ─── Markdown fix pipeline ───────────────────────────────────────────────────
// Gemini outputs often contain CJK text with ** bold markers that fail
// CommonMark flanking rules (e.g. **"引号"**, **连续****加粗**).
//
// Strategy: protect code blocks → convert all **...** to
// <strong>...</strong> HTML (bypasses flanking entirely) → restore code.

export function fixMarkdown(content: string): string {
  let text = content
    // Strip Gemini internal iemoji: markers, keep the code value
    .replace(/iemoji:([^:\s)]{1,20})/g, "$1")
    // Escape currency $ (e.g. $300) so remarkMath doesn't treat them as math delimiters.
    .replace(/(?<!\$)\$(\d)/g, "\\$$1");

  // Protect fenced & inline code from bold replacement
  const codeSlots: string[] = [];
  const codePH = (i: number) => `\x00C${i}\x00`;
  text = text.replace(/```[\s\S]*?```/g, (m) => {
    codeSlots.push(m);
    return codePH(codeSlots.length - 1);
  });
  text = text.replace(/`[^`]+`/g, (m) => {
    codeSlots.push(m);
    return codePH(codeSlots.length - 1);
  });

  // Convert **...** to <strong>...</strong> (non-greedy, bypasses CommonMark flanking)
  text = text.replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>");

  // Restore code blocks
  text = text.replace(/\x00C(\d+)\x00/g, (_, i) => codeSlots[parseInt(i)]);

  return text;
}

// ─── Timeline constants ────────────────────────────────────────────────────
const TL_PAD = 18;       // top/bottom padding inside long canvas (px)
const TL_MIN_GAP = 18;   // minimum vertical gap between dots (px)
const TL_BAR_WIDTH = 25; // total bar width (px)
const TL_BAR_RIGHT = 8;  // bar distance from right edge of parent (px)
const TL_HIT = 20;       // dot hit-area size (px)
const TL_DOT = 9;        // normal dot visual diameter (px)
const TL_DOT_ACTIVE = 9; // active dot visual diameter (px)

// ─── Timeline utility functions ────────────────────────────────────────────

/** Three-pass min-gap enforcement (forward → backward → forward). */
function applyMinGap(
  positions: number[],
  minTop: number,
  maxTop: number,
  gap: number,
): number[] {
  const n = positions.length;
  if (n === 0) return positions;
  const out = positions.slice();

  out[0] = Math.max(minTop, Math.min(out[0], maxTop));
  for (let i = 1; i < n; i++) {
    out[i] = Math.max(positions[i], out[i - 1] + gap);
  }

  if (out[n - 1] > maxTop) {
    out[n - 1] = maxTop;
    for (let i = n - 2; i >= 0; i--) {
      out[i] = Math.min(out[i], out[i + 1] - gap);
    }
    if (out[0] < minTop) {
      out[0] = minTop;
      for (let i = 1; i < n; i++) {
        out[i] = Math.max(out[i], out[i - 1] + gap);
      }
    }
  }

  for (let i = 0; i < n; i++) {
    out[i] = Math.max(minTop, Math.min(maxTop, out[i]));
  }
  return out;
}

/** First index where arr[i] >= x. */
function lowerBound(arr: number[], x: number): number {
  let lo = 0, hi = arr.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (arr[mid] < x) lo = mid + 1; else hi = mid;
  }
  return lo;
}

/** Last index where arr[i] <= x. */
function upperBound(arr: number[], x: number): number {
  let lo = 0, hi = arr.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (arr[mid] <= x) lo = mid + 1; else hi = mid;
  }
  return lo - 1;
}

// ─── ConversationTimeline ──────────────────────────────────────────────────

interface TimelineProps {
  messages: ConvMessage[];
  scrollerEl: HTMLElement | null;
  visibleRange: { startIndex: number; endIndex: number };
  onJumpTo: (globalIndex: number) => void;
}

interface HoveredInfo {
  localIdx: number;
  /** screen-space Y of the dot center */
  screenY: number;
  /** screen-space X of the bar's left edge (tooltip anchor) */
  barLeft: number;
}

function ConversationTimeline({ messages, scrollerEl, visibleRange, onJumpTo }: TimelineProps) {
  const t = useTheme();
  const barRef = useRef<HTMLDivElement>(null);
  // Long-canvas inner div; moved via translateY (no scroll container = no scrollbar artifact).
  const innerRef = useRef<HTMLDivElement>(null);
  // Current timeline offset in px (mirrors innerRef transform).
  const offsetRef = useRef(0);
  const [barHeight, setBarHeight] = useState(0);
  const [dotRange, setDotRange] = useState({ start: 0, end: -1 });
  const [hovered, setHovered] = useState<{ info: HoveredInfo; visible: boolean } | null>(null);
  const tooltipTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // ── Refs holding latest geometry so event listeners stay stable ────────
  const yPositionsRef = useRef<number[]>([]);
  const barHeightRef = useRef(0);
  const contentHeightRef = useRef(0);

  // ── Collect user messages with global indices ──────────────────────────
  const userMsgs = useMemo(() => {
    const result: { globalIndex: number; text: string }[] = [];
    for (let i = 0; i < messages.length; i++) {
      if (messages[i].role === "user") {
        result.push({ globalIndex: i, text: messages[i].text });
      }
    }
    return result;
  }, [messages]);

  const N = userMsgs.length;

  // ── Long-canvas geometry ───────────────────────────────────────────────
  const { contentHeight, yPositions } = useMemo((): {
    contentHeight: number;
    yPositions: number[];
  } => {
    if (N === 0 || barHeight === 0) return { contentHeight: barHeight, yPositions: [] };
    const needed = 2 * TL_PAD + Math.max(0, N - 1) * TL_MIN_GAP;
    const ch = Math.max(barHeight, Math.ceil(needed));
    const usableC = Math.max(1, ch - 2 * TL_PAD);
    const desired = userMsgs.map((_, i) =>
      TL_PAD + (N <= 1 ? 0 : i / (N - 1)) * usableC,
    );
    const yPos = applyMinGap(desired, TL_PAD, TL_PAD + usableC, TL_MIN_GAP);
    return { contentHeight: ch, yPositions: yPos };
  }, [N, barHeight, userMsgs]);

  // Keep refs in sync with latest geometry values
  useEffect(() => { yPositionsRef.current = yPositions; }, [yPositions]);
  useEffect(() => { barHeightRef.current = barHeight; }, [barHeight]);
  useEffect(() => { contentHeightRef.current = contentHeight; }, [contentHeight]);

  // ── Active local index ─────────────────────────────────────────────────
  const activeLocalIdx = useMemo(() => {
    if (N === 0) return 0;
    const mid = Math.round((visibleRange.startIndex + visibleRange.endIndex) / 2);
    let idx = 0;
    for (let i = 0; i < N; i++) {
      if (userMsgs[i].globalIndex <= mid) idx = i;
      else break;
    }
    return idx;
  }, [visibleRange, userMsgs, N]);

  // ── Stable helper: recompute dotRange from a given scrollTop ──────────
  // Uses refs so this function never changes reference, keeping listeners stable.
  const updateDotRange = React.useCallback((scrollTop: number) => {
    const yPos = yPositionsRef.current;
    const bh = barHeightRef.current;
    if (yPos.length === 0 || bh === 0) return;
    const buffer = Math.max(100, bh);
    const s = lowerBound(yPos, scrollTop - buffer);
    const e = Math.max(s - 1, upperBound(yPos, scrollTop + bh + buffer));
    setDotRange(prev => prev.start === s && prev.end === e ? prev : { start: s, end: e });
  }, []);

  // ── Cleanup tooltip fade-out timer on unmount ─────────────────────────
  useEffect(() => () => { if (tooltipTimerRef.current) clearTimeout(tooltipTimerRef.current); }, []);

  // ── ResizeObserver for bar height ──────────────────────────────────────
  useEffect(() => {
    const el = barRef.current;
    if (!el) return;
    const h0 = el.clientHeight;
    if (h0 > 0) setBarHeight(h0);
    const ro = new ResizeObserver(([entry]) => {
      const h = entry.contentRect.height;
      if (h > 0) setBarHeight(h);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // ── Recompute visible dots when geometry changes ───────────────────────
  useEffect(() => {
    if (yPositions.length === 0 || barHeight === 0) return;
    updateDotRange(offsetRef.current);
  }, [yPositions, barHeight, updateDotRange]);

  // ── Wheel capture on bar: independent timeline scroll ─────────────────
  useEffect(() => {
    const bar = barRef.current;
    if (!bar) return;
    const handleWheel = (e: WheelEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const ch = contentHeightRef.current;
      const bh = barHeightRef.current;
      const maxOffset = Math.max(0, ch - bh);
      const newOffset = Math.max(0, Math.min(maxOffset, offsetRef.current + e.deltaY));
      if (Math.abs(newOffset - offsetRef.current) > 0.5) {
        offsetRef.current = newOffset;
        if (innerRef.current) {
          innerRef.current.style.transform = `translateY(-${newOffset}px)`;
        }
        updateDotRange(newOffset);
      }
    };
    bar.addEventListener("wheel", handleWheel, { passive: false });
    return () => bar.removeEventListener("wheel", handleWheel);
  }, [updateDotRange]);

  // ── Main scroller → translateY sync ───────────────────────────────────
  useEffect(() => {
    if (!scrollerEl) return;
    let rafId: number | null = null;
    const sync = () => {
      rafId = null;
      const inner = innerRef.current;
      if (!inner) return;
      const bh = barHeightRef.current;
      const ch = contentHeightRef.current;
      if (bh === 0 || yPositionsRef.current.length === 0) return;
      const { scrollTop, scrollHeight, clientHeight } = scrollerEl;
      const maxMain = Math.max(1, scrollHeight - clientHeight);
      const ratio = Math.max(0, Math.min(1, scrollTop / maxMain));
      const maxTimeline = Math.max(0, ch - bh);
      const target = Math.round(ratio * maxTimeline);
      if (Math.abs(offsetRef.current - target) > 1) {
        offsetRef.current = target;
        inner.style.transform = `translateY(-${target}px)`;
        updateDotRange(target);
      }
    };
    const onScroll = () => { if (rafId !== null) return; rafId = requestAnimationFrame(sync); };
    scrollerEl.addEventListener("scroll", onScroll, { passive: true });
    sync();
    return () => { scrollerEl.removeEventListener("scroll", onScroll); if (rafId !== null) cancelAnimationFrame(rafId); };
  }, [scrollerEl, updateDotRange]);

  // ── Inject CSS once: hide scrollbar + dot hover / focus styles ─────────
  useEffect(() => {
    const id = "conv-timeline-styles";
    if (document.getElementById(id)) return;
    const style = document.createElement("style");
    style.id = id;
    style.textContent = `
      .conv-tl-dot { outline: none; }
      .conv-tl-dot:hover .conv-tl-pip { transform: scale(1.55) !important; }
      .conv-tl-dot:focus-visible .conv-tl-pip {
        outline: 2px solid #0071e3; outline-offset: 3px;
      }
      @keyframes conv-tl-tooltip-in {
        from { opacity: 0; transform: translateY(-50%) translateX(6px); }
        to   { opacity: 1; transform: translateY(-50%) translateX(0); }
      }
      @keyframes conv-tl-tooltip-out {
        from { opacity: 1; transform: translateY(-50%) translateX(0); }
        to   { opacity: 0; transform: translateY(-50%) translateX(6px); }
      }
    `;
    document.head.appendChild(style);
  }, []);

  if (N === 0) return null;

  const dotColor = t.isDark ? "rgba(255,255,255,0.30)" : "rgba(0,0,0,0.22)";

  // Tooltip text: first ~150 chars of the hovered user message
  const tooltipText = hovered !== null
    ? (userMsgs[hovered.info.localIdx]?.text ?? "").trim().replace(/\s+/g, " ").slice(0, 150)
    : "";

  return (
    <>
      {/* ── Floating frosted-glass bar (absolute, overlaid) ── */}
      <div
        ref={barRef}
        style={{
          position: "absolute",
          // Hug the right edge; dots center at parentRight-18px,
          // which clears the 20px message padding zone (no text underneath).
          right: TL_BAR_RIGHT,
          top: 0,
          bottom: 0,
          width: TL_BAR_WIDTH,
          // Fully transparent — only the dots themselves are visible.
          background: "transparent",
          zIndex: 10,
          overflow: "hidden",
        }}
      >
        {/* ── Long-canvas inner div, moved via translateY (no scroll container) ── */}
        <div
          ref={innerRef}
          style={{
            position: "absolute",
            top: 0,
            left: 0,
            width: "100%",
            height: contentHeight,
            willChange: "transform",
          }}
        >
          {userMsgs.slice(dotRange.start, dotRange.end + 1).map((msg, i) => {
              const localIdx = dotRange.start + i;
              const y = yPositions[localIdx];
              const isActive = localIdx === activeLocalIdx;
              const dotSize = isActive ? TL_DOT_ACTIVE : TL_DOT;

              return (
                <button
                  key={msg.globalIndex}
                  className="conv-tl-dot"
                  aria-label={`跳转到：${msg.text.slice(0, 40)}`}
                  onClick={() => onJumpTo(msg.globalIndex)}
                  onMouseEnter={(e) => {
                    if (tooltipTimerRef.current) clearTimeout(tooltipTimerRef.current);
                    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
                    const barRect = barRef.current?.getBoundingClientRect();
                    setHovered({
                      info: {
                        localIdx,
                        screenY: rect.top + rect.height / 2,
                        barLeft: barRect?.left ?? rect.left,
                      },
                      visible: true,
                    });
                  }}
                  onMouseLeave={() => {
                    setHovered(prev => prev ? { ...prev, visible: false } : null);
                    tooltipTimerRef.current = setTimeout(() => setHovered(null), 180);
                  }}
                  style={{
                    position: "absolute",
                    top: y,
                    left: "50%",
                    transform: "translate(-50%, -50%)",
                    width: TL_HIT,
                    height: TL_HIT,
                    border: "none",
                    background: "transparent",
                    cursor: "pointer",
                    padding: 0,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    borderRadius: "50%",
                  }}
                >
                  <span
                    className="conv-tl-pip"
                    style={{
                      display: "block",
                      width: dotSize,
                      height: dotSize,
                      borderRadius: "50%",
                      background: isActive ? "#0071e3" : dotColor,
                      boxShadow: isActive
                        ? "0 0 0 2.5px #0071e340, 0 0 8px #0071e360"
                        : "none",
                      transition:
                        "width 0.15s ease, height 0.15s ease, background 0.15s ease, box-shadow 0.15s ease, transform 0.15s ease",
                      flexShrink: 0,
                    }}
                  />
                </button>
              );
            })}
        </div>
      </div>

      {/* ── Tooltip card (position:fixed — not clipped by parent overflow) ── */}
      {hovered !== null && tooltipText && (
        <div
          style={{
            position: "fixed",
            // Fixed offset from viewport right: bar occupies [TL_BAR_RIGHT, TL_BAR_RIGHT+TL_BAR_WIDTH], +6px gap
            right: TL_BAR_RIGHT + TL_BAR_WIDTH + 6,
            // center on the dot's Y, clamped to stay on screen
            top: Math.max(8, Math.min(
              window.innerHeight - 120,
              hovered.info.screenY,
            )),
            // Symmetric fade in/out: same keyframe magnitude both directions
            animation: hovered.visible
              ? "conv-tl-tooltip-in 0.16s ease forwards"
              : "conv-tl-tooltip-out 0.16s ease forwards",
            zIndex: 1000,
            maxWidth: 240,
            pointerEvents: "none",
            background: t.isDark
              ? "rgba(20,23,30,0.92)"
              : "rgba(255,255,255,0.94)",
            backdropFilter: "blur(18px) saturate(120%)",
            WebkitBackdropFilter: "blur(18px) saturate(120%)",
            borderRadius: 10,
            border: t.isDark
              ? "1px solid rgba(255,255,255,0.13)"
              : "1px solid rgba(0,0,0,0.09)",
            padding: "9px 13px",
            fontSize: 12.5,
            lineHeight: 1.55,
            color: t.text,
            boxShadow: "0 6px 24px rgba(0,0,0,0.22)",
            wordBreak: "break-word",
            whiteSpace: "pre-wrap",
            overflow: "hidden",
          }}
        >
          {tooltipText}
        </div>
      )}
    </>
  );
}

export function markdownCodeLanguage(className?: string): string {
  const matched = /language-([\w-]+)/.exec(className || "");
  return matched?.[1] || "text";
}

export function MarkdownCodeBlock({
  code,
  language,
  isDark,
}: {
  code: string;
  language: string;
  isDark: boolean;
}) {
  const [copied, setCopied] = useState(false);

  function handleCopy() {
    void navigator.clipboard.writeText(code)
      .then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 900);
      })
      .catch((e) => {
        console.error("复制代码失败:", e);
      });
  }

  return (
    <div style={{ position: "relative", margin: "0.6em 0" }}>
      <button
        onClick={handleCopy}
        title={copied ? "已复制" : "复制代码"}
        style={{
          position: "absolute",
          top: 8,
          right: 8,
          zIndex: 2,
          border: "none",
          borderRadius: 6,
          padding: "2px 8px",
          fontSize: 11,
          background: copied
            ? "rgba(22,163,74,0.9)"
            : (isDark ? "rgba(15,23,42,0.7)" : "rgba(255,255,255,0.85)"),
          color: copied ? "#fff" : (isDark ? "#dbeafe" : "#334155"),
          borderColor: isDark ? "rgba(148,163,184,0.25)" : "rgba(148,163,184,0.45)",
          borderStyle: "solid",
          borderWidth: 1,
          cursor: "pointer",
        }}
      >
        {copied ? "已复制" : "复制"}
      </button>
      <SyntaxHighlighter
        language={language}
        style={isDark ? vscDarkPlus : oneLight}
        customStyle={{
          margin: 0,
          borderRadius: 8,
          padding: "12px 14px",
          fontSize: 12.5,
          lineHeight: 1.6,
          background: isDark ? "#121826" : "#f8fafc",
        }}
        codeTagProps={{
          style: {
            fontFamily: "\"SF Mono\", \"Fira Code\", monospace",
          },
        }}
      >
        {code}
      </SyntaxHighlighter>
    </div>
  );
}

// Format ISO 8601 timestamp to "HH:MM"
function formatMsgTime(iso: string): string {
  try {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso;
    const hh = String(d.getHours()).padStart(2, "0");
    const mm = String(d.getMinutes()).padStart(2, "0");
    return `${hh}:${mm}`;
  } catch {
    return iso;
  }
}

export function formatProgressBits(opts: {
  rounds: number;
  webCount: number;
  thinkingCount: number;
  entryCount: number;
}): string[] {
  const bits: string[] = [];
  if (opts.rounds > 0) bits.push(`${opts.rounds} rounds`);
  if (opts.webCount > 0) bits.push(`${opts.webCount.toLocaleString()} sites`);
  if (opts.thinkingCount > 0) bits.push(`${opts.thinkingCount} thinkings`);
  if (opts.entryCount > 0) bits.push(`${opts.entryCount.toLocaleString()} records`);
  return bits;
}

export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "0 B";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1048576).toFixed(1)} MB`;
}

// Format ISO 8601 to "YYYY-MM-DD"
function formatMsgDate(iso: string): string {
  try {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso;
    const y = d.getFullYear();
    const m = String(d.getMonth() + 1).padStart(2, "0");
    const day = String(d.getDate()).padStart(2, "0");
    return `${y}-${m}-${day}`;
  } catch {
    return iso;
  }
}

interface ChatViewProps {
  conversation: Conversation | null;
  accountId?: string;
  mediaDir?: string;  // path to accounts/{id}/media/
  mediaVersion?: number;
  scrollToMessageId?: string | null;
  onScrolledToMessage?: () => void;
}

export function ChatView({ conversation, accountId, mediaDir, mediaVersion = 0, scrollToMessageId, onScrolledToMessage }: ChatViewProps) {
  const t = useTheme();
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const [scrollerEl, setScrollerEl] = useState<HTMLElement | null>(null);
  const [visibleRange, setVisibleRange] = useState({ startIndex: 0, endIndex: 0 });
  const [highlightedMessageId, setHighlightedMessageId] = useState<string | null>(null);
  const [researchModal, setResearchModal] = useState<ResearchModalState | null>(null);
  const highlightTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const scrollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onScrolledToMessageRef = useRef(onScrolledToMessage);
  useEffect(() => { onScrolledToMessageRef.current = onScrolledToMessage; }, [onScrolledToMessage]);
  // 记录上一次 effect 执行后的对话 id（独立 effect 保证 scroll effect 先读到旧值，再由本 effect 更新）
  const mountedConvIdRef = useRef<string | undefined>(undefined);
  const parseWarning =
    conversation && typeof conversation.parseWarning === "string" && conversation.parseWarning.trim()
      ? conversation.parseWarning.trim()
      : "";

  const visibleMessages = useMemo(() => {
    if (!conversation) return [];
    return conversation.messages.filter((msg) => !msg.hidden);
  }, [conversation]);

  // 搜索跳转：对话加载后滚动到目标消息
  useEffect(() => {
    if (!scrollToMessageId || !conversation || visibleMessages.length === 0) return;
    const idx = visibleMessages.findIndex((m) => m.id === scrollToMessageId);
    if (idx < 0) return; // 消息不在当前对话中，等待正确对话加载后再触发

    // mountedConvIdRef 由下方独立 effect 维护，此处读到的是上一次渲染后的值
    const isNewConv = mountedConvIdRef.current !== conversation.id;
    const targetId = scrollToMessageId;

    const doScroll = () => {
      virtuosoRef.current?.scrollToIndex({ index: idx, behavior: "auto", align: "center" });
      if (highlightTimerRef.current) clearTimeout(highlightTimerRef.current);
      setHighlightedMessageId(targetId);
      highlightTimerRef.current = setTimeout(() => setHighlightedMessageId(null), 1000);
      onScrolledToMessageRef.current?.();
    };

    if (scrollTimerRef.current) clearTimeout(scrollTimerRef.current);
    if (isNewConv) {
      // 新对话：Virtuoso 刚挂载，initialTopMostItemIndex 已定位，给更多时间让列表渲染完毕再校正
      scrollTimerRef.current = setTimeout(doScroll, 200);
    } else {
      // 同一对话：Virtuoso 已挂载，rAF 即可
      requestAnimationFrame(doScroll);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scrollToMessageId, conversation, visibleMessages]);

  // 独立 effect：在 scroll effect 执行完毕后更新 mountedConvIdRef，供下次判断
  // 必须定义在 scroll effect 之后，确保 React 先执行 scroll effect 读到旧值，再由本 effect 更新
  useEffect(() => {
    mountedConvIdRef.current = conversation?.id;
  }, [conversation]);

  if (!conversation) {
    return (
      <div style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", background: "transparent" }}>
        <div style={{ textAlign: "center", color: t.textMuted }}>
          <div style={{ fontSize: 44, marginBottom: 10 }}>💬</div>
          <div style={{ fontSize: 15, fontWeight: 600, color: t.text, marginBottom: 5 }}>选择一个对话</div>
          <div style={{ fontSize: 13 }}>从左侧列表中选择对话查看内容</div>
        </div>
      </div>
    );
  }

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", background: "transparent", overflow: "hidden" }}>
      {parseWarning && (
        <div
          style={{
            margin: "8px 14px 0",
            padding: "8px 10px",
            borderRadius: 8,
            fontSize: 12,
            color: t.isDark ? "#ffd28a" : "#9a5b00",
            background: t.isDark ? "rgba(255,173,51,0.14)" : "rgba(255,173,51,0.15)",
            border: t.isDark ? "1px solid rgba(255,173,51,0.35)" : "1px solid rgba(255,173,51,0.4)",
          }}
        >
          {parseWarning}
        </div>
      )}
      {(() => {
        return visibleMessages.length === 0 ? (
          <div style={{ textAlign: "center", color: t.textMuted, fontSize: 13, marginTop: 60 }}>暂无消息记录</div>
        ) : (
          // position:relative so the absolutely-positioned timeline bar can anchor to it
          <div style={{ flex: 1, position: "relative", overflow: "hidden" }}>
            <Virtuoso
              ref={virtuosoRef}
              scrollerRef={(ref) => {
                if (ref instanceof HTMLElement) {
                  ref.setAttribute("data-tl-scroller", "");
                  setScrollerEl(ref);
                } else {
                  setScrollerEl(null);
                }
              }}
              rangeChanged={setVisibleRange}
              key={`${conversation.id}:${conversation.updatedAt}:${mediaVersion}`}
              data={visibleMessages}
              followOutput="smooth"
              initialTopMostItemIndex={scrollToMessageId ? Math.max(0, visibleMessages.findIndex((m) => m.id === scrollToMessageId)) : visibleMessages.length - 1}
              itemContent={(_, msg) => (
                <MessageBubble
                  message={msg}
                  accountId={accountId}
                  mediaDir={mediaDir}
                  cacheKey={`${conversation.id}:${conversation.updatedAt}:${mediaVersion}`}
                  isHighlighted={msg.id === highlightedMessageId}
                  onOpenResearch={setResearchModal}
                />
              )}
              style={{ position: "absolute", inset: 0 }}
            />
            <ConversationTimeline
              messages={visibleMessages}
              scrollerEl={scrollerEl}
              visibleRange={visibleRange}
              onJumpTo={(idx) =>
                virtuosoRef.current?.scrollToIndex({ index: idx, behavior: "smooth", align: "start" })
              }
            />
          </div>
        );
      })()}
      <ResearchDetailModal state={researchModal} onClose={() => setResearchModal(null)} />
    </div>
  );
}

function AttachmentStrip({
  attachments,
  mediaDir,
  cacheKey,
  alignRight,
}: {
  attachments: Attachment[];
  mediaDir?: string;
  cacheKey: string;
  alignRight: boolean;
}) {
  const [lightboxIdx, setLightboxIdx] = useState<number | null>(null);
  const [filePreviewIdx, setFilePreviewIdx] = useState<number | null>(null);
  const failedAttachments = attachments.filter((a) => a.downloadFailed);
  const renderableAttachments = attachments.filter((a) => !a.downloadFailed);
  // 音乐对检测：Gemini 对音乐同时输出一个 video/*（封面合并版）和一个 audio/*，
  // 实际代表同一首音乐，只显示视频封面（带播放按钮），隐藏重复的音频文件卡片。
  const isMusicPair = renderableAttachments.length === 2
    && renderableAttachments.some((a) => a.mimeType.startsWith("video/"))
    && renderableAttachments.some((a) => a.mimeType.startsWith("audio/"));
  const fileAttachments = useMemo(
    () => isMusicPair
      ? []
      : renderableAttachments.filter((a) => getKind(a.mimeType) === "file"),
    [renderableAttachments, isMusicPair],
  );
  const mediaAttachmentsBase = useMemo(
    () => dedupeLikelyFormatVariants(
      isMusicPair
        ? renderableAttachments.filter((a) => a.mimeType.startsWith("video/"))
        : renderableAttachments.filter((a) => getKind(a.mimeType) !== "file"),
    ),
    [renderableAttachments, isMusicPair],
  );
  const mediaKey = useMemo(
    () => mediaAttachmentsBase.map((a) => `${a.mediaId}:${a.mimeType}`).join("|"),
    [mediaAttachmentsBase],
  );
  const [collapseTwinImages, setCollapseTwinImages] = useState(false);
  const mediaAttachments = collapseTwinImages ? [mediaAttachmentsBase[0]] : mediaAttachmentsBase;

  React.useEffect(() => {
    let cancelled = false;
    setCollapseTwinImages(false);

    const imageAttachments = mediaAttachmentsBase.filter((a) => getKind(a.mimeType) === "image");
    if (imageAttachments.length !== 2 || mediaAttachmentsBase.length !== 2) return () => { cancelled = true; };

    const leftUrl = buildUrl(imageAttachments[0].mediaId, mediaDir, cacheKey);
    const rightUrl = buildUrl(imageAttachments[1].mediaId, mediaDir, cacheKey);
    if (!leftUrl || !rightUrl) return () => { cancelled = true; };

    (async () => {
      const [leftHash, rightHash] = await Promise.all([
        computeImageDHash(leftUrl),
        computeImageDHash(rightUrl),
      ]);
      if (cancelled || !leftHash || !rightHash) return;
      if (hammingDistance(leftHash, rightHash) <= 2) {
        setCollapseTwinImages(true);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [mediaKey, mediaDir, cacheKey]);

  return (
    <>
      {failedAttachments.length > 0 && (
        <div
          style={{
            fontSize: 11,
            color: "#d97706",
            marginBottom: 6,
            opacity: 0.9,
            textAlign: alignRight ? "right" : "left",
          }}
        >
          {failedAttachments.length} 个附件下载失败，点击同步可重试
        </div>
      )}
      {/* Media thumbnails */}
      {mediaAttachments.length > 0 && (
        <div style={{ display: "flex", flexWrap: "wrap", gap: 6, justifyContent: alignRight ? "flex-end" : "flex-start", marginBottom: 6 }}>
          {mediaAttachments.map((att, i) => {
            const url = buildUrl(att.mediaId, mediaDir, cacheKey);
            const kind = getKind(att.mimeType);
            if (kind === "image") return (
              <ImageThumbnail
                key={i}
                url={url}
                alt={att.mediaId}
                onClick={() => setLightboxIdx(i)}
              />
            );
            if (kind === "audio") return (
              <div
                key={i}
                onClick={() => setLightboxIdx(i)}
                style={{ width: 160, height: 110, borderRadius: 14, overflow: "hidden", cursor: "pointer", flexShrink: 0, background: "linear-gradient(135deg, #1a1a2e 0%, #16213e 100%)", boxShadow: "0 2px 8px rgba(0,0,0,0.3)", position: "relative", display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 6 }}
              >
                <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="rgba(255,255,255,0.7)" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M9 18V5l12-2v13" />
                  <circle cx="6" cy="18" r="3" />
                  <circle cx="18" cy="16" r="3" />
                </svg>
                <div style={{ fontSize: 10, color: "rgba(255,255,255,0.5)", maxWidth: 140, textAlign: "center", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", padding: "0 8px" }}>
                  {att.mediaId}
                </div>
              </div>
            );
            return (
              <div
                key={i}
                onClick={() => setLightboxIdx(i)}
                style={{ width: 160, height: 110, borderRadius: 14, overflow: "hidden", cursor: "pointer", flexShrink: 0, background: "#111", boxShadow: "0 2px 8px rgba(0,0,0,0.3)", position: "relative" }}
              >
                <VideoThumbnail videoUrl={url} />
                <div style={{ position: "absolute", inset: 0, display: "flex", alignItems: "center", justifyContent: "center", background: "rgba(0,0,0,0.35)" }}>
                  <div style={{ width: 36, height: 36, borderRadius: "50%", border: "1.5px solid rgba(255,255,255,0.85)", display: "flex", alignItems: "center", justifyContent: "center" }}>
                    <svg width="12" height="12" viewBox="0 0 16 16" fill="rgba(255,255,255,0.9)">
                      <polygon points="5,2 14,8 5,14" />
                    </svg>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}


      {/* File attachments */}
      {fileAttachments.length > 0 && (
        <div style={{ display: "flex", flexWrap: "wrap", gap: 6, justifyContent: alignRight ? "flex-end" : "flex-start", marginBottom: 6 }}>
          {fileAttachments.map((att, i) => {
            const ext = att.mediaId.split(".").pop()?.toUpperCase() || "FILE";
            const displayName = att.mimeType.split("/").pop() || ext;
            const isTextFile = att.mimeType.startsWith("text/");
            return (
              <div
                key={`file-${i}`}
                onClick={isTextFile ? () => setFilePreviewIdx(i) : undefined}
                style={{
                  width: 160, height: 110, borderRadius: 14, overflow: "hidden", flexShrink: 0,
                  background: "#1a1a2e", boxShadow: "0 2px 8px rgba(0,0,0,0.3)",
                  display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
                  gap: 6, padding: "8px 10px",
                  cursor: isTextFile ? "pointer" : "default",
                }}
              >
                <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="rgba(255,255,255,0.6)" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                  <polyline points="14 2 14 8 20 8" />
                </svg>
                <div style={{ fontSize: 11, color: "rgba(255,255,255,0.7)", textAlign: "center", lineHeight: 1.3, wordBreak: "break-all", maxHeight: 28, overflow: "hidden" }}>
                  {displayName}
                </div>
                <div style={{ fontSize: 9, color: "rgba(255,255,255,0.35)", textTransform: "uppercase" }}>
                  {ext}
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* Lightbox — portal 到 body 避免父级 transform/filter 影响 fixed 定位 */}
      {lightboxIdx !== null && ReactDOM.createPortal(
        <LightboxModal
          attachments={mediaAttachments}
          index={lightboxIdx}
          mediaDir={mediaDir}
          cacheKey={cacheKey}
          onClose={() => setLightboxIdx(null)}
          onChange={setLightboxIdx}
        />,
        document.body,
      )}

      {/* File preview — portal 到 body 避免父级 transform/filter 影响 fixed 定位 */}
      {filePreviewIdx !== null && ReactDOM.createPortal(
        <FilePreviewModal
          attachment={fileAttachments[filePreviewIdx]}
          mediaDir={mediaDir}
          onClose={() => setFilePreviewIdx(null)}
        />,
        document.body,
      )}
    </>
  );
}

function ImageThumbnail({
  url,
  alt,
  onClick,
}: {
  url: string;
  alt: string;
  onClick: () => void;
}) {
  const t = useTheme();
  const imgRef = React.useRef<HTMLImageElement | null>(null);
  const [loading, setLoading] = useState(() => !!url && !loadedImageUrlCache.has(url));

  React.useEffect(() => {
    if (!url || loadedImageUrlCache.has(url)) {
      setLoading(false);
      return;
    }
    setLoading(true);
  }, [url]);

  React.useEffect(() => {
    const img = imgRef.current;
    if (!img || !url) return;
    if (img.complete && img.naturalWidth > 0) {
      loadedImageUrlCache.add(url);
      setLoading(false);
    }
  }, [url]);

  return (
    <div
      onClick={onClick}
      style={{
        width: 120,
        height: 120,
        borderRadius: 14,
        overflow: "hidden",
        cursor: "pointer",
        flexShrink: 0,
        background: t.isDark ? "#1a1a1c" : "#d9d9dc",
        boxShadow: "0 2px 8px rgba(0,0,0,0.25)",
        position: "relative",
      }}
    >
      <img
        ref={imgRef}
        src={url}
        alt={alt}
        onLoad={() => {
          if (url) loadedImageUrlCache.add(url);
          setLoading(false);
        }}
        onError={() => setLoading(false)}
        style={{
          width: "100%",
          height: "100%",
          objectFit: "cover",
          display: "block",
          opacity: loading ? 0.62 : 1,
          transition: "opacity 0.22s ease-out",
        }}
        draggable={false}
      />
      {loading && (
        <div
          style={{
            position: "absolute",
            inset: 0,
            pointerEvents: "none",
            background: t.isDark ? "rgba(0,0,0,0.15)" : "rgba(255,255,255,0.2)",
          }}
        >
          <div
            style={{
              position: "absolute",
              inset: 0,
              backgroundImage: t.isDark
                ? "repeating-linear-gradient(135deg, rgba(255,255,255,0.03) 0 16px, rgba(255,255,255,0.12) 16px 32px, rgba(255,255,255,0.03) 32px 48px)"
                : "repeating-linear-gradient(135deg, rgba(255,255,255,0.10) 0 16px, rgba(255,255,255,0.30) 16px 32px, rgba(255,255,255,0.10) 32px 48px)",
              backgroundSize: "260px 260px",
              animation: "mediaLoadingDiagonalSweep 3.2s linear infinite",
              opacity: t.isDark ? 0.55 : 0.5,
              willChange: "background-position",
            }}
          />
        </div>
      )}
    </div>
  );
}

/** 利用 WebView 原生解码能力，通过 <video> + <canvas> 抽取视频首帧作为预览图。 */
const videoFrameCache = new Map<string, string>();

function VideoThumbnail({ videoUrl }: { videoUrl: string }) {
  const [frameDataUrl, setFrameDataUrl] = useState<string | null>(
    () => videoFrameCache.get(videoUrl) ?? null,
  );
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (frameDataUrl || failed || !videoUrl) return;
    if (videoFrameCache.has(videoUrl)) {
      setFrameDataUrl(videoFrameCache.get(videoUrl)!);
      return;
    }

    let cancelled = false;
    const video = document.createElement("video");
    video.crossOrigin = "anonymous";
    video.muted = true;
    video.preload = "auto";
    video.playsInline = true;

    const cleanup = () => {
      video.removeAttribute("src");
      video.load();
    };

    video.addEventListener("loadeddata", () => {
      if (cancelled) return;
      // 拨到 0.1s 以跳过可能的纯黑首帧
      video.currentTime = 0.1;
    }, { once: true });

    video.addEventListener("seeked", () => {
      if (cancelled) return;
      try {
        const canvas = document.createElement("canvas");
        canvas.width = video.videoWidth;
        canvas.height = video.videoHeight;
        const ctx = canvas.getContext("2d");
        if (ctx) {
          ctx.drawImage(video, 0, 0);
          const dataUrl = canvas.toDataURL("image/jpeg", 0.75);
          videoFrameCache.set(videoUrl, dataUrl);
          setFrameDataUrl(dataUrl);
        }
      } catch {
        setFailed(true);
      }
      cleanup();
    }, { once: true });

    video.addEventListener("error", () => {
      if (!cancelled) setFailed(true);
      cleanup();
    }, { once: true });

    video.src = videoUrl;

    return () => {
      cancelled = true;
      cleanup();
    };
  }, [videoUrl, frameDataUrl, failed]);

  if (frameDataUrl) {
    return (
      <img
        src={frameDataUrl}
        alt="video preview"
        style={{ width: "100%", height: "100%", objectFit: "cover", display: "block" }}
        draggable={false}
      />
    );
  }

  // fallback: 灰底占位
  return (
    <div style={{ width: "100%", height: "100%", background: "#222", display: "flex", alignItems: "center", justifyContent: "center" }}>
      {!failed && (
        <>
          <style>{`@keyframes spin{to{transform:rotate(360deg)}}`}</style>
          <div style={{ width: 16, height: 16, border: "2px solid rgba(255,255,255,0.2)", borderTop: "2px solid rgba(255,255,255,0.6)", borderRadius: "50%", animation: "spin 0.8s linear infinite" }} />
        </>
      )}
    </div>
  );
}

const FILE_PREVIEW_MAX_BYTES = 128 * 1024; // 128 KB

function FilePreviewModal({
  attachment,
  mediaDir,
  onClose,
}: {
  attachment: Attachment;
  mediaDir?: string;
  onClose: () => void;
}) {
  const [content, setContent] = useState<string | null>(null);
  const [truncated, setTruncated] = useState(false);
  const [error, setError] = useState<string | null>(null);

  React.useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  React.useEffect(() => {
    if (!mediaDir || !attachment.mediaId) return;
    const url = buildUrl(attachment.mediaId, mediaDir);
    fetch(url)
      .then(async (r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        const buf = await r.arrayBuffer();
        const isTruncated = buf.byteLength > FILE_PREVIEW_MAX_BYTES;
        const slice = isTruncated ? buf.slice(0, FILE_PREVIEW_MAX_BYTES) : buf;
        const text = new TextDecoder("utf-8", { fatal: false }).decode(slice);
        setContent(text);
        setTruncated(isTruncated);
      })
      .catch((e) => setError(e.message));
  }, [attachment.mediaId, mediaDir]);

  // 从原始文件名推断显示名称
  const origName = attachment.mediaId.includes("-")
    ? attachment.mimeType.split("/").pop() || attachment.mediaId
    : attachment.mediaId;

  return (
    <div
      onClick={onClose}
      style={{ position: "fixed", inset: 0, zIndex: 1000, background: "rgba(0,0,0,0.85)", display: "flex", alignItems: "center", justifyContent: "center" }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          position: "relative", width: "80vw", maxWidth: 800, maxHeight: "85vh",
          background: "#1e1e2e", borderRadius: 12, display: "flex", flexDirection: "column", overflow: "hidden",
        }}
      >
        {/* Header */}
        <div style={{ padding: "12px 16px", borderBottom: "1px solid rgba(255,255,255,0.08)", display: "flex", alignItems: "center", justifyContent: "space-between", flexShrink: 0 }}>
          <span style={{ fontSize: 13, color: "rgba(255,255,255,0.7)", fontWeight: 500 }}>{origName}</span>
          <button
            onClick={onClose}
            style={{ width: 28, height: 28, borderRadius: "50%", background: "rgba(255,255,255,0.1)", border: "none", color: "#fff", fontSize: 16, cursor: "pointer", display: "flex", alignItems: "center", justifyContent: "center" }}
          >×</button>
        </div>

        {/* Content */}
        <div style={{ flex: 1, overflow: "auto", padding: "16px 20px" }}>
          {error ? (
            <div style={{ color: "#f87171", fontSize: 13 }}>读取失败: {error}</div>
          ) : content === null ? (
            <div style={{ color: "rgba(255,255,255,0.4)", fontSize: 13 }}>加载中...</div>
          ) : (
            <>
              <pre style={{
                margin: 0, whiteSpace: "pre-wrap", wordBreak: "break-word",
                fontSize: 12, lineHeight: 1.6, color: "rgba(255,255,255,0.82)",
                fontFamily: "ui-monospace, 'SF Mono', Menlo, Monaco, 'Cascadia Code', monospace",
              }}>
                {content}
              </pre>
              {truncated && (
                <div style={{ marginTop: 12, padding: "8px 0", borderTop: "1px solid rgba(255,255,255,0.08)", color: "rgba(255,255,255,0.4)", fontSize: 11, textAlign: "center" }}>
                  文件内容过长，仅展示前 128 KB
                </div>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function LightboxModal({
  attachments,
  index,
  mediaDir,
  cacheKey,
  onClose,
  onChange,
}: {
  attachments: Attachment[];
  index: number;
  mediaDir?: string;
  cacheKey: string;
  onClose: () => void;
  onChange: (i: number) => void;
}) {
  const att = attachments[index];
  const url = buildUrl(att.mediaId, mediaDir, cacheKey);
  const kind = getKind(att.mimeType);

  React.useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      if (e.key === "ArrowLeft" && index > 0) onChange(index - 1);
      if (e.key === "ArrowRight" && index < attachments.length - 1) onChange(index + 1);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [index, attachments.length]);

  return (
    <div
      onClick={onClose}
      style={{ position: "fixed", inset: 0, zIndex: 1000, background: "rgba(0,0,0,0.85)", display: "flex", alignItems: "center", justifyContent: "center" }}
    >
      <div onClick={(e) => e.stopPropagation()} style={{ position: "relative", maxWidth: "90vw", maxHeight: "90vh" }}>
        {kind === "image" ? (
          <img
            src={url}
            alt={att.mediaId}
            style={{ maxWidth: "90vw", maxHeight: "90vh", borderRadius: 12, objectFit: "contain", display: "block" }}
          />
        ) : kind === "audio" ? (
          <div style={{ background: "rgba(255,255,255,0.08)", borderRadius: 12, padding: "32px 40px", display: "flex", flexDirection: "column", alignItems: "center", gap: 16, minWidth: 320 }}>
            <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="rgba(255,255,255,0.6)" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M9 18V5l12-2v13" />
              <circle cx="6" cy="18" r="3" />
              <circle cx="18" cy="16" r="3" />
            </svg>
            <div style={{ color: "rgba(255,255,255,0.7)", fontSize: 13, maxWidth: 280, textAlign: "center", wordBreak: "break-all" }}>
              {att.mediaId}
            </div>
            <audio src={url} controls autoPlay style={{ width: "100%" }} />
          </div>
        ) : (
          <video
            src={url}
            controls
            autoPlay
            style={{ maxWidth: "90vw", maxHeight: "90vh", borderRadius: 12, display: "block" }}
          />
        )}

        {/* Close button */}
        <button
          onClick={onClose}
          style={{ position: "absolute", top: -16, right: -16, width: 32, height: 32, borderRadius: "50%", background: "rgba(255,255,255,0.15)", border: "none", color: "#fff", fontSize: 18, cursor: "pointer", display: "flex", alignItems: "center", justifyContent: "center" }}
        >×</button>

        {index > 0 && (
          <button
            onClick={() => onChange(index - 1)}
            style={{ position: "absolute", left: -48, top: "50%", transform: "translateY(-50%)", width: 36, height: 36, borderRadius: "50%", background: "rgba(255,255,255,0.15)", border: "none", color: "#fff", fontSize: 20, cursor: "pointer", display: "flex", alignItems: "center", justifyContent: "center" }}
          >‹</button>
        )}
        {index < attachments.length - 1 && (
          <button
            onClick={() => onChange(index + 1)}
            style={{ position: "absolute", right: -48, top: "50%", transform: "translateY(-50%)", width: 36, height: 36, borderRadius: "50%", background: "rgba(255,255,255,0.15)", border: "none", color: "#fff", fontSize: 20, cursor: "pointer", display: "flex", alignItems: "center", justifyContent: "center" }}
          >›</button>
        )}

        {attachments.length > 1 && (
          <div style={{ position: "absolute", bottom: -30, left: "50%", transform: "translateX(-50%)", color: "rgba(255,255,255,0.6)", fontSize: 13 }}>
            {index + 1} / {attachments.length}
          </div>
        )}
      </div>
    </div>
  );
}

// ── Canvas / Deep Research bubbles ─────────────────────────────────────────
// 设计原则：不做"塞进气泡的按钮卡片"。Plan/Report 直接复用 AI 气泡外壳（同
// aiBubbleBg、同圆角、同阴影）；Canvas 做成消息下方的"附件条"。所有可点击区域
// 去明显 border，仅 hover 时用 Apple 蓝极淡 tint，不做 scale 形变。

const ACCENT_BLUE = "#0071e3";
const ACCENT_BLUE_DARK = "#9cc9ff";

// AI markdown 正文渲染（复用于 AI 气泡、plan/report 气泡内部 header）
function AIMarkdown({ text, isDark }: { text: string; isDark: boolean }) {
  return (
    <div className={`prose-ai${isDark ? " prose-dark" : ""}`}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeRaw, rehypeKatex]}
        components={{
          a: ({ href, children, ...props }) => (
            <a
              {...props}
              href={href}
              target="_blank"
              rel="noopener noreferrer"
              onClick={(e) => {
                e.preventDefault();
                if (!href) return;
                void openUrl(href);
              }}
            >
              {children}
            </a>
          ),
          pre: ({ children }) => <>{children}</>,
          code: ({ className, children, ...props }) => {
            const content = String(children ?? "");
            const isBlock =
              (className || "").includes("language-") || content.includes("\n");
            if (!isBlock) {
              return (
                <code className={className} {...props}>
                  {children}
                </code>
              );
            }
            return (
              <MarkdownCodeBlock
                code={content.replace(/\n$/, "")}
                language={markdownCodeLanguage(className)}
                isDark={isDark}
              />
            );
          },
        }}
      >
        {fixMarkdown(text)}
      </ReactMarkdown>
    </div>
  );
}

function aiShellStyle(t: ReturnType<typeof useTheme>): React.CSSProperties {
  return {
    background: t.aiBubbleBg,
    borderRadius: "18px 18px 18px 6px",
    boxShadow: t.isDark ? "0 1px 3px rgba(0,0,0,0.3)" : "0 1px 3px rgba(0,0,0,0.07)",
    overflow: "hidden",
  };
}

function rowHoverBg(t: ReturnType<typeof useTheme>) {
  return t.isDark ? "rgba(124,167,255,0.10)" : "rgba(0,113,227,0.06)";
}

// Apple 蓝 halo dot：外圈淡底 + 内实心点，避免扁平感。
function HaloDot({ t, filled = true }: { t: ReturnType<typeof useTheme>; filled?: boolean }) {
  const accent = t.isDark ? ACCENT_BLUE_DARK : ACCENT_BLUE;
  return (
    <div
      style={{
        width: 16,
        height: 16,
        borderRadius: "50%",
        background: t.isDark ? "rgba(124,167,255,0.18)" : "rgba(0,113,227,0.15)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        flexShrink: 0,
      }}
    >
      <div
        style={{
          width: 7,
          height: 7,
          borderRadius: "50%",
          background: filled ? accent : "transparent",
          border: filled ? "none" : `1.5px solid ${accent}`,
        }}
      />
    </div>
  );
}

type CanvasItem = NonNullable<ConvMessage["canvas"]>[number];

/** Canvas 文件行：inline=true 时不带外壳（嵌入 contentBlocks 中使用）。 */
function CanvasBubble({
  canvas,
  mediaDir,
  leadingText,
  inline,
}: {
  canvas: CanvasItem;
  mediaDir?: string;
  leadingText?: string;
  inline?: boolean;
}) {
  const t = useTheme();
  const [hovered, setHovered] = useState(false);
  const absPath = mediaDir && canvas.content_media_id ? `${mediaDir}/${canvas.content_media_id}` : "";
  const disabled = !absPath;
  const bytes = canvas.size_bytes ?? 0;
  const metaBits: string[] = [];
  if (bytes > 0) metaBits.push(formatBytes(bytes));
  const metaText = metaBits.join(" · ");
  const hasLeading = !inline && !!(leadingText && leadingText.trim().length > 0);

  async function handleOpen() {
    if (disabled) return;
    const fileUrl = `file://${absPath}`;
    try {
      await openUrl(fileUrl);
    } catch (err) {
      console.error("openUrl(file://) failed, fallback to openPath:", err);
      try {
        await openPath(absPath);
      } catch (err2) {
        console.error("openPath fallback failed:", err2);
      }
    }
  }

  const accent = t.isDark ? ACCENT_BLUE_DARK : ACCENT_BLUE;
  const rowBg = hovered && !disabled ? rowHoverBg(t) : "transparent";

  const btn = (
    <button
      type="button"
      onClick={handleOpen}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      disabled={disabled}
      title={disabled ? "media 文件缺失" : `在默认浏览器中打开 ${canvas.filename}`}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 12,
        width: "100%",
        padding: "10px 16px",
        border: "none",
        borderRadius: 0,
        background: rowBg,
        cursor: disabled ? "default" : "pointer",
        textAlign: "left",
        color: t.text,
        transition: "background 0.15s",
        opacity: disabled ? 0.55 : 1,
      }}
    >
      <div
        style={{
          width: 36,
          height: 36,
          borderRadius: 9,
          background: t.isDark ? "rgba(124,167,255,0.12)" : "rgba(0,113,227,0.08)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          flexShrink: 0,
        }}
      >
        <DocIcon color={accent} size={18} />
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 13, fontWeight: 600, color: t.text, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {canvas.filename || canvas.title || "canvas"}
        </div>
        <div style={{ fontSize: 11, color: t.textSub, marginTop: 2, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {canvas.title && canvas.title !== canvas.filename ? canvas.title : ""}
          {canvas.title && canvas.title !== canvas.filename && metaText ? " · " : ""}
          {metaText}
        </div>
      </div>
      <ExternalLinkIcon color={hovered && !disabled ? accent : t.textMuted} />
    </button>
  );

  if (inline) return btn;

  return (
    <div style={aiShellStyle(t)}>
      {hasLeading && (
        <div style={{ padding: "14px 18px 8px", fontSize: 14, lineHeight: 1.55, color: t.text, wordBreak: "break-word" }}>
          <AIMarkdown text={leadingText!} isDark={t.isDark} />
        </div>
      )}
      {btn}
    </div>
  );
}

function ResearchPlanBubble({
  plan,
  leadingText,
}: {
  plan: NonNullable<ConvMessage["deepResearch"]>;
  leadingText?: string;
}) {
  const t = useTheme();
  const steps = plan.steps ?? [];
  const railColor = t.isDark ? "rgba(255,255,255,0.12)" : "rgba(0,0,0,0.08)";
  const accent = t.isDark ? ACCENT_BLUE_DARK : ACCENT_BLUE;
  const hasLeading = !!(leadingText && leadingText.trim().length > 0);

  return (
    <div style={{ ...aiShellStyle(t), padding: "14px 18px 16px" }}>
      {/* 引导正文（如"我已经更新了方案..."），仅靠 spacing 与计划区自然分层，不画分割线 */}
      {hasLeading && (
        <div style={{ fontSize: 14, lineHeight: 1.55, color: t.text, wordBreak: "break-word", marginBottom: 14 }}>
          <AIMarkdown text={leadingText!} isDark={t.isDark} />
        </div>
      )}

      {/* 小号 accent 标签 + 标题 */}
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 12 }}>
        <div style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 4,
          fontSize: 10,
          fontWeight: 600,
          letterSpacing: 0.4,
          padding: "2px 7px",
          borderRadius: 999,
          background: t.isDark ? "rgba(124,167,255,0.14)" : "rgba(0,113,227,0.10)",
          color: accent,
          textTransform: "uppercase",
        }}>
          <SparkIcon color={accent} size={10} />
          研究计划
        </div>
        {plan.title && (
          <div style={{ fontSize: 13.5, fontWeight: 600, color: t.text, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {plan.title}
          </div>
        )}
      </div>

      {/* 垂直时间线：dot 在左，name/description 在右 */}
      <div style={{ position: "relative" }}>
        {/* 左侧连贯竖线 —— 延伸覆盖所有 step 中心 */}
        {steps.length > 1 && (
          <div
            style={{
              position: "absolute",
              left: 7.5, // HaloDot 宽度 16 的中心
              top: 14,
              bottom: 14,
              width: 1,
              background: railColor,
            }}
          />
        )}
        {steps.map((s, i) => {
          const hasDesc = !!(s.description && s.description.trim().length > 0);
          return (
            <div
              key={i}
              style={{
                display: "flex",
                gap: 12,
                alignItems: "flex-start",
                padding: i === 0 ? "0 0 10px" : i === steps.length - 1 ? "10px 0 0" : "10px 0",
                position: "relative",
              }}
            >
              {/* dot 容器给竖线让路，用外壳同色背景遮盖线段 */}
              <div style={{ position: "relative", zIndex: 1, paddingTop: 2, background: t.aiBubbleBg }}>
                <HaloDot t={t} />
              </div>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 13, fontWeight: 600, color: t.text, lineHeight: 1.4 }}>
                  {s.name}
                </div>
                {hasDesc && (
                  <div
                    style={{
                      marginTop: 6,
                      fontSize: 12.5,
                      color: t.textSub,
                      lineHeight: 1.65,
                      whiteSpace: "pre-wrap",
                      wordBreak: "break-word",
                    }}
                  >
                    {s.description}
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>

      {plan.estimated_time && (
        <div style={{ marginTop: 12, fontSize: 11, color: t.textMuted }}>{plan.estimated_time}</div>
      )}
    </div>
  );
}

function ResearchReportBubble({
  report,
  leadingText,
  accountId,
  mediaDir,
  onOpenResearch,
}: {
  report: NonNullable<ConvMessage["deepResearch"]>;
  leadingText?: string;
  accountId?: string;
  mediaDir?: string;
  onOpenResearch?: (state: ResearchModalState) => void;
}) {
  const t = useTheme();
  const [progressHovered, setProgressHovered] = useState(false);
  const [reportHovered, setReportHovered] = useState(false);
  const accent = t.isDark ? ACCENT_BLUE_DARK : ACCENT_BLUE;
  const hasLeading = !!(leadingText && leadingText.trim().length > 0);

  const rounds = report.rounds ?? 0;
  const webCount = report.web_count ?? 0;
  const fileCount = report.file_count ?? 0;
  const entryCount = report.entry_count ?? 0;
  const sourceCount = webCount + fileCount;
  const progressBits: string[] = [];
  if (rounds > 0) progressBits.push(`${rounds} 轮`);
  if (sourceCount > 0) progressBits.push(`${sourceCount} 个来源`);
  const progressMeta = progressBits.join(" · ") || (entryCount > 0 ? `${entryCount} 条记录` : "无调研记录");
  const progressDisabled = !report.progress_media_id;

  const chars = report.char_count ?? 0;
  const bytes = report.size_bytes ?? 0;
  const reportBits: string[] = [];
  if (chars > 0) reportBits.push(`${chars.toLocaleString()} 字`);
  if (bytes > 0) reportBits.push(formatBytes(bytes));
  const reportMeta = reportBits.join(" · ");
  const reportDisabled = !report.report_media_id;

  const openResearch = (defaultTab: "progress" | "report") => {
    if (!accountId || !onOpenResearch) return;
    onOpenResearch({
      accountId,
      mediaDir,
      title: report.title || "研究报告",
      reportMediaId: report.report_media_id,
      progressMediaId: report.progress_media_id,
      charCount: report.char_count,
      sizeBytes: report.size_bytes,
      rounds: report.rounds,
      webCount: report.web_count,
      fileCount: report.file_count,
      thinkingCount: report.thinking_count,
      entryCount: report.entry_count,
      defaultTab,
    });
  };


  function row(opts: {
    icon: React.ReactNode;
    label: string;
    main: string;
    meta: string;
    hovered: boolean;
    setHovered: (v: boolean) => void;
    disabled: boolean;
    onClick: () => void;
    title?: string;
  }) {
    return (
      <button
        type="button"
        onClick={opts.onClick}
        onMouseEnter={() => opts.setHovered(true)}
        onMouseLeave={() => opts.setHovered(false)}
        disabled={opts.disabled}
        title={opts.title}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          width: "100%",
          padding: "12px 16px",
          border: "none",
          background: opts.hovered && !opts.disabled ? rowHoverBg(t) : "transparent",
          cursor: opts.disabled ? "default" : "pointer",
          textAlign: "left",
          color: t.text,
          transition: "background 0.15s",
          opacity: opts.disabled ? 0.55 : 1,
        }}
      >
        <div style={{
          flex: "0 0 auto",
          width: 30,
          height: 30,
          borderRadius: 8,
          background: t.isDark ? "rgba(124,167,255,0.12)" : "rgba(0,113,227,0.08)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}>
          {opts.icon}
        </div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: 10.5, color: t.textMuted, letterSpacing: 0.4, marginBottom: 2, textTransform: "uppercase" }}>
            {opts.label}
          </div>
          <div style={{ fontSize: 13, fontWeight: 600, color: t.text, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {opts.main}
          </div>
          {opts.meta && (
            <div style={{ fontSize: 11, color: t.textSub, marginTop: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {opts.meta}
            </div>
          )}
        </div>
        <ChevronRightIcon color={opts.hovered && !opts.disabled ? accent : t.textMuted} />
      </button>
    );
  }

  return (
    <div style={aiShellStyle(t)}>
      {hasLeading && (
        <div style={{ padding: "14px 18px 6px", fontSize: 14, lineHeight: 1.55, color: t.text, wordBreak: "break-word" }}>
          <AIMarkdown text={leadingText!} isDark={t.isDark} />
        </div>
      )}
      {!progressDisabled && row({
        icon: <SearchIcon color={accent} />,
        label: "调研过程",
        main: progressMeta,
        meta: progressBits.length === 2 ? `共 ${entryCount} 条记录` : "",
        hovered: progressHovered,
        setHovered: setProgressHovered,
        disabled: !accountId,
        onClick: () => openResearch("progress"),
        title: accountId ? "查看调研过程" : "账号信息缺失",
      })}
      {row({
        icon: <DocIcon color={accent} />,
        label: "报告详情",
        main: report.title || "研究报告",
        meta: reportMeta,
        hovered: reportHovered,
        setHovered: setReportHovered,
        disabled: reportDisabled || !accountId,
        onClick: () => openResearch("report"),
        title: reportDisabled ? "报告正文缺失" : !accountId ? "账号信息缺失" : "查看报告详情",
      })}
    </div>
  );
}

function MessageBubble({
  message,
  accountId,
  mediaDir,
  cacheKey,
  isHighlighted,
  onOpenResearch,
}: {
  message: ConvMessage;
  accountId?: string;
  mediaDir?: string;
  cacheKey: string;
  isHighlighted?: boolean;
  onOpenResearch?: (state: ResearchModalState) => void;
}) {
  const t = useTheme();
  const [copiedId, setCopiedId] = useState(false);
  const isUser = message.role === "user";
  const hasText = (message.text || "").trim().length > 0;
  const dr = message.deepResearch;
  const isPlan = !isUser && dr?.type === "plan";
  const isReport = !isUser && dr?.type === "report";
  const canvasList = (!isUser && message.canvas) || [];
  const hasCanvas = canvasList.length > 0;
  const contentBlocks = (!isUser && message.contentBlocks) || [];
  const hasBlocks = contentBlocks.length > 0;
  // plan / report 正文融入自身气泡壳；有 contentBlocks 时由 blocks 渲染（含交错 canvas）。
  const showText = hasText && !isPlan && !isReport && !hasBlocks && !hasCanvas;
  const attachmentsBlock = message.attachments.length > 0 ? (
    <AttachmentStrip
      attachments={message.attachments}
      mediaDir={mediaDir}
      cacheKey={cacheKey}
      alignRight={isUser}
    />
  ) : null;

  const copyIdBtnColor = t.isDark ? "rgba(255,255,255,0.22)" : "rgba(0,0,0,0.18)";
  const copyIdBtn = (
    <button
      onClick={() => {
        void navigator.clipboard.writeText(message.id).then(() => {
          setCopiedId(true);
          setTimeout(() => setCopiedId(false), 850);
        });
      }}
      title={copiedId ? "已复制" : "复制消息 ID"}
      style={{ width: 14, height: 14, borderRadius: 4, border: "none", background: "transparent", cursor: "pointer", display: "inline-flex", alignItems: "center", justifyContent: "center", flexShrink: 0, padding: 0, transition: "background 0.15s", opacity: 0.7 }}
      onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = t.btnHoverBg; (e.currentTarget as HTMLElement).style.opacity = "1"; }}
      onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; (e.currentTarget as HTMLElement).style.opacity = "0.7"; }}
    >
      {copiedId ? <CheckIcon color="#16a34a" /> : <CopyIcon color={copyIdBtnColor} />}
    </button>
  );

  return (
    <div style={{ display: "flex", justifyContent: isUser ? "flex-end" : "flex-start", padding: "4px 26px 4px 20px", gap: 8 }}>
      <div style={{ maxWidth: isUser ? "62%" : "94%" }}>
        {isUser && attachmentsBlock}
        {showText && (
          <div style={{
            padding: isUser ? "10px 14px" : "12px 16px",
            borderRadius: isUser ? "18px 18px 6px 18px" : "18px 18px 18px 6px",
            background: isUser ? "linear-gradient(135deg, #0071e3 0%, #0077ed 100%)" : t.aiBubbleBg,
            color: isUser ? "#fff" : t.text,
            fontSize: 14,
            lineHeight: 1.55,
            boxShadow: isHighlighted
              ? (isUser ? "0 0 0 2px rgba(0,113,227,0.8), 0 0 16px 4px rgba(0,113,227,0.5)" : t.isDark ? "0 0 0 2px rgba(99,179,255,0.7), 0 0 16px 4px rgba(99,179,255,0.35)" : "0 0 0 2px rgba(0,113,227,0.6), 0 0 16px 4px rgba(0,113,227,0.25)")
              : isUser ? "0 2px 8px rgba(0,113,227,0.22)" : t.isDark ? "0 1px 3px rgba(0,0,0,0.3)" : "0 1px 3px rgba(0,0,0,0.07)",
            transition: "box-shadow 0.3s ease",
            wordBreak: "break-word",
          }}>
            {isUser ? (
              <span style={{ whiteSpace: "pre-wrap" }}>{message.text}</span>
            ) : (
              <AIMarkdown text={message.text} isDark={t.isDark} />
            )}
          </div>
        )}
        {isPlan && dr && (
          <ResearchPlanBubble plan={dr} leadingText={hasText ? message.text : undefined} />
        )}
        {isReport && dr && (
          <ResearchReportBubble
            report={dr}
            leadingText={hasText ? message.text : undefined}
            accountId={accountId}
            mediaDir={mediaDir}
            onOpenResearch={onOpenResearch}
          />
        )}
        {hasBlocks && (
          <div style={aiShellStyle(t)}>
            {contentBlocks.map((block, bi) =>
              block.kind === "text" ? (
                <div key={bi} style={{ padding: "12px 18px", fontSize: 14, lineHeight: 1.55, color: t.text, wordBreak: "break-word" }}>
                  <AIMarkdown text={block.text} isDark={t.isDark} />
                </div>
              ) : canvasList[block.canvas_index] ? (
                <CanvasBubble
                  key={bi}
                  canvas={canvasList[block.canvas_index]}
                  mediaDir={mediaDir}
                  inline
                />
              ) : null,
            )}
          </div>
        )}
        {hasCanvas && !hasBlocks && canvasList.map((cv, ci) => (
          <CanvasBubble
            key={ci}
            canvas={cv}
            mediaDir={mediaDir}
            leadingText={ci === 0 && hasText ? message.text : undefined}
          />
        ))}
        {!isUser && attachmentsBlock}
        <div style={{ fontSize: 11, color: t.textMuted, marginTop: (showText || isPlan || isReport || hasCanvas) ? 3 : 1, textAlign: isUser ? "right" : "left", padding: "0 4px", display: "flex", gap: 4, justifyContent: isUser ? "flex-end" : "flex-start", alignItems: "center", flexWrap: "wrap" }}>
          {isUser && copyIdBtn}
          <span>{formatMsgDate(message.timestamp)} {formatMsgTime(message.timestamp)}</span>
          {!isUser && (
            <>
              <span style={{ opacity: 0.4 }}>·</span>
              <span style={{ color: t.textSub }}>{message.genMeta?.model || message.model || "未知模型"}</span>
              {copyIdBtn}
              {message.attachments.length > 0 && (() => {
                const atts = message.attachments;
                // 音乐文件：Gemini 对音乐同时输出一个 video/* (封面合并版) 和一个 audio/*，
                // 实际代表同一首音乐，计为 audio ×1。
                const hasVideo = atts.some((a) => a.mimeType.startsWith("video/"));
                const hasAudio = atts.some((a) => a.mimeType.startsWith("audio/"));
                const isMusicPair = atts.length === 2 && hasVideo && hasAudio;
                let displayCount: number;
                let mediaType: string;
                if (isMusicPair) {
                  mediaType = "audio";
                  displayCount = 1;
                } else {
                  const first = atts[0];
                  if (first.mimeType.startsWith("video/")) mediaType = "video";
                  else if (first.mimeType.startsWith("audio/")) mediaType = "audio";
                  else if (first.mimeType.startsWith("image/")) mediaType = "image";
                  else mediaType = "file";
                  displayCount = atts.length;
                }
                const countText = displayCount > 1 ? `${mediaType} ×${displayCount}` : mediaType;
                // 累加附件体积（来自 Rust 注入的 size 字段，单位 bytes）
                const totalBytes = atts.reduce((sum, a) => sum + (a.size ?? 0), 0);
                const sizeText = totalBytes > 0
                  ? ` · ${(totalBytes / 1048576).toFixed(1)} MB`
                  : "";
                return (
                  <span style={{
                    fontSize: 10,
                    fontWeight: 500,
                    color: t.textMuted,
                    background: t.hover,
                    borderRadius: 4,
                    padding: "1px 5px",
                    marginLeft: 5,
                    letterSpacing: 0.2,
                  }}>
                    {countText}{sizeText}
                  </span>
                );
              })()}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
