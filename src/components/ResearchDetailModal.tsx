import React, { useEffect, useMemo, useRef, useState } from "react";
import ReactDOM from "react-dom";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
import rehypeRaw from "rehype-raw";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useTheme } from "../theme";
import {
  CloseIcon,
  SparkIcon,
  DocIcon,
  GlobeIcon,
  SpinnerIcon,
} from "./Icons";
import {
  fixMarkdown,
  markdownCodeLanguage,
  MarkdownCodeBlock,
  formatBytes,
  formatProgressBits,
} from "./ChatView";

const ACCENT_BLUE = "#0071e3";
const ACCENT_BLUE_DARK = "#9cc9ff";

type ProgressEntry = {
  type: "thinking" | "web_search" | "file_search";
  title?: string;
  description?: string;
  round?: number;
  url?: string;
  page_title?: string;
  filename?: string;
};

export type ResearchModalState = {
  accountId: string;
  mediaDir?: string;
  title: string;
  reportMediaId?: string;
  progressMediaId?: string;
  charCount?: number;
  sizeBytes?: number;
  rounds?: number;
  webCount?: number;
  fileCount?: number;
  thinkingCount?: number;
  entryCount?: number;
  defaultTab: "progress" | "report";
};

interface Props {
  state: ResearchModalState | null;
  onClose: () => void;
}

export function ResearchDetailModal({ state, onClose }: Props) {
  const t = useTheme();
  const [tab, setTab] = useState<"progress" | "report">("report");

  useEffect(() => {
    if (state) setTab(state.defaultTab);
  }, [state]);

  useEffect(() => {
    if (!state) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [state, onClose]);

  if (!state) return null;

  const accent = t.isDark ? ACCENT_BLUE_DARK : ACCENT_BLUE;
  const hasProgress = !!state.progressMediaId;
  const hasReport = !!state.reportMediaId;

  const overlay: React.CSSProperties = {
    position: "fixed",
    inset: 0,
    background: t.isDark ? "rgba(0,0,0,0.62)" : "rgba(20,24,32,0.38)",
    backdropFilter: "blur(6px)",
    WebkitBackdropFilter: "blur(6px)",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    zIndex: 1000,
    padding: 24,
  };
  const card: React.CSSProperties = {
    width: "min(1200px, 92vw)",
    height: "min(880px, 88vh)",
    background: t.isDark ? "#1c1f25" : "#ffffff",
    borderRadius: 16,
    boxShadow: t.isDark
      ? "0 20px 60px rgba(0,0,0,0.55)"
      : "0 20px 60px rgba(20,30,55,0.22)",
    border: `1px solid ${t.divider}`,
    display: "flex",
    flexDirection: "column",
    overflow: "hidden",
    color: t.text,
  };

  function tabBtn(key: "progress" | "report", label: string, disabled: boolean) {
    const active = tab === key;
    return (
      <button
        type="button"
        disabled={disabled}
        onClick={() => setTab(key)}
        style={{
          background: "transparent",
          border: "none",
          padding: "10px 14px",
          fontSize: 13,
          fontWeight: 600,
          color: active ? accent : disabled ? t.textMuted : t.textSub,
          cursor: disabled ? "default" : "pointer",
          opacity: disabled ? 0.5 : 1,
          position: "relative",
          transition: "color 0.15s",
        }}
      >
        {label}
        {active && (
          <div
            style={{
              position: "absolute",
              left: 10,
              right: 10,
              bottom: -1,
              height: 2,
              background: accent,
              borderRadius: 1,
            }}
          />
        )}
      </button>
    );
  }

  return ReactDOM.createPortal(
    <div style={overlay} onClick={onClose}>
      <div style={card} onClick={(e) => e.stopPropagation()}>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            padding: "10px 14px 0 18px",
            borderBottom: `1px solid ${t.divider}`,
            gap: 8,
            flex: "0 0 auto",
          }}
        >
          <div style={{ display: "flex", alignItems: "baseline", gap: 10, minWidth: 0, flex: 1 }}>
            <SparkIcon color={accent} size={14} />
            <div
              style={{
                fontSize: 14,
                fontWeight: 600,
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
                minWidth: 0,
              }}
            >
              {state.title || "研究报告"}
            </div>
            {(() => {
              const bits: string[] = [];
              if (state.charCount && state.charCount > 0) bits.push(`${state.charCount.toLocaleString()} 字`);
              if (state.sizeBytes && state.sizeBytes > 0) bits.push(formatBytes(state.sizeBytes));
              if (bits.length === 0) return null;
              return (
                <div style={{ fontSize: 12, color: t.textMuted, whiteSpace: "nowrap", flexShrink: 0 }}>
                  {bits.join(" · ")}
                </div>
              );
            })()}
          </div>
          <div style={{ display: "flex", alignItems: "flex-end", gap: 2 }}>
            {tabBtn("progress", "调研过程", !hasProgress)}
            {tabBtn("report", "报告详情", !hasReport)}
          </div>
          <div style={{ width: 8 }} />
          <button
            type="button"
            onClick={onClose}
            aria-label="关闭"
            style={{
              background: "transparent",
              border: "none",
              padding: 6,
              borderRadius: 8,
              cursor: "pointer",
              color: t.textSub,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
            onMouseEnter={(e) => (e.currentTarget.style.background = t.btnHoverBg)}
            onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
          >
            <CloseIcon color={t.textSub} size={16} />
          </button>
        </div>

        <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
          {tab === "report" && hasReport && (
            <ReportPanel
              accountId={state.accountId}
              reportMediaId={state.reportMediaId!}
            />
          )}
          {tab === "progress" && hasProgress && (
            <ProgressPanel
              accountId={state.accountId}
              progressMediaId={state.progressMediaId!}
              rounds={state.rounds}
              webCount={state.webCount}
              thinkingCount={state.thinkingCount}
              entryCount={state.entryCount}
            />
          )}
        </div>
      </div>
    </div>,
    document.body,
  );
}

// ─── Report Panel ─────────────────────────────────────────────────────────

interface TocItem {
  level: number;
  text: string;
  id: string;
}

/** 预扫 markdown 文本，按 heading 出现顺序产出 TOC（跳过 fenced code block 内部的 #）。 */
function buildToc(md: string): TocItem[] {
  const lines = md.split(/\r?\n/);
  const items: TocItem[] = [];
  let inFence = false;
  let fenceTag = "";
  let idx = 0;
  for (const line of lines) {
    const fence = line.match(/^\s{0,3}(```+|~~~+)/);
    if (fence) {
      if (!inFence) {
        inFence = true;
        fenceTag = fence[1];
      } else if (line.trimStart().startsWith(fenceTag)) {
        inFence = false;
      }
      continue;
    }
    if (inFence) continue;
    const m = line.match(/^\s{0,3}(#{1,3})\s+(.+?)\s*#*\s*$/);
    if (!m) continue;
    const level = m[1].length;
    const text = m[2].replace(/`/g, "").trim();
    if (!text) continue;
    items.push({ level, text, id: `h-${idx}` });
    idx += 1;
  }
  return items;
}

function ReportPanel({
  accountId,
  reportMediaId,
}: {
  accountId: string;
  reportMediaId: string;
}) {
  const t = useTheme();
  const accent = t.isDark ? ACCENT_BLUE_DARK : ACCENT_BLUE;
  const [md, setMd] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let cancelled = false;
    setMd(null);
    setErr(null);
    (async () => {
      try {
        const txt = await invoke<string>("read_media_file", {
          accountId,
          mediaId: reportMediaId,
        });
        if (!cancelled) setMd(txt);
      } catch (e: any) {
        if (!cancelled) setErr(String(e?.message ?? e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [accountId, reportMediaId]);

  const toc = useMemo(() => (md ? buildToc(md) : []), [md]);

  useEffect(() => {
    if (!md || toc.length === 0) return;
    const scroller = scrollRef.current;
    if (!scroller) return;
    let io: IntersectionObserver | null = null;
    const timer = setTimeout(() => {
      const headingEls = Array.from(
        scroller.querySelectorAll<HTMLElement>("[data-toc-id]"),
      );
      if (headingEls.length === 0) return;
      io = new IntersectionObserver(
        (entries) => {
          const visible = entries
            .filter((e) => e.isIntersecting)
            .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0];
          if (visible) {
            const id = (visible.target as HTMLElement).dataset.tocId;
            if (id) setActiveId(id);
          }
        },
        { root: scroller, rootMargin: "0px 0px -75% 0px", threshold: [0, 1] },
      );
      headingEls.forEach((el) => io!.observe(el));
      setActiveId((prev) => prev ?? headingEls[0].dataset.tocId ?? null);
    }, 30);
    return () => {
      clearTimeout(timer);
      io?.disconnect();
    };
  }, [md, toc.length]);

  // heading id 生成计数器（与 buildToc 顺序一致）
  let hCount = 0;
  const nextHeadingId = () => `h-${hCount++}`;

  const onTocClick = (id: string) => {
    const scroller = scrollRef.current;
    if (!scroller) return;
    const el = scroller.querySelector<HTMLElement>(`[data-toc-id="${id}"]`);
    if (!el) return;
    const top = el.offsetTop - 12;
    scroller.scrollTo({ top, behavior: "smooth" });
    setActiveId(id);
  };

  return (
    <div style={{ display: "flex", flex: 1, minHeight: 0, minWidth: 0 }}>
      {/* 左栏 TOC */}
      <div
        style={{
          width: 260,
          flex: "0 0 auto",
          borderRight: `1px solid ${t.divider}`,
          padding: "16px 8px 16px 16px",
          overflowY: "auto",
          fontSize: 12.5,
        }}
      >
        <div
          style={{
            fontSize: 10.5,
            letterSpacing: 0.6,
            textTransform: "uppercase",
            color: t.textMuted,
            padding: "2px 10px",
            marginBottom: 8,
          }}
        >
          目录
        </div>
        {toc.length === 0 && md !== null && (
          <div style={{ padding: "4px 10px", color: t.textMuted, fontSize: 12 }}>
            本报告无小节标题
          </div>
        )}
        {toc.map((item) => {
          const active = activeId === item.id;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => onTocClick(item.id)}
              title={item.text}
              style={{
                display: "block",
                width: "100%",
                textAlign: "left",
                border: "none",
                background: active ? (t.isDark ? "rgba(124,167,255,0.12)" : "rgba(0,113,227,0.08)") : "transparent",
                color: active ? accent : item.level === 1 ? t.text : t.textSub,
                padding: `6px 10px 6px ${10 + (item.level - 1) * 14}px`,
                borderRadius: 6,
                cursor: "pointer",
                fontSize: item.level === 1 ? 13 : 12.5,
                fontWeight: item.level === 1 ? 600 : active ? 600 : 400,
                lineHeight: 1.4,
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
                marginBottom: 1,
              }}
              onMouseEnter={(e) => {
                if (!active) e.currentTarget.style.background = t.hover;
              }}
              onMouseLeave={(e) => {
                if (!active) e.currentTarget.style.background = "transparent";
              }}
            >
              {item.text}
            </button>
          );
        })}
      </div>

      {/* 右栏 markdown */}
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
        <div
          ref={scrollRef}
          style={{
            flex: 1,
            overflowY: "auto",
            padding: "24px 36px 80px",
            scrollBehavior: "smooth",
          }}
        >
          {md === null && !err && (
            <div style={{ display: "flex", alignItems: "center", gap: 8, color: t.textMuted, fontSize: 13 }}>
              <SpinnerIcon color={t.textMuted} />
              正在加载报告…
            </div>
          )}
          {err && (
            <div style={{ color: "#d84a3a", fontSize: 13 }}>加载失败：{err}</div>
          )}
          {md !== null && !err && (
            <div style={{ maxWidth: 780, margin: "0 auto" }} className={`prose-ai${t.isDark ? " prose-dark" : ""}`}>
              <ReactMarkdown
                remarkPlugins={[remarkGfm, remarkMath]}
                rehypePlugins={[rehypeRaw, rehypeKatex]}
                components={{
                  h1: ({ children, ...props }) => {
                    const id = nextHeadingId();
                    return <h1 id={id} data-toc-id={id} {...props}>{children}</h1>;
                  },
                  h2: ({ children, ...props }) => {
                    const id = nextHeadingId();
                    return <h2 id={id} data-toc-id={id} {...props}>{children}</h2>;
                  },
                  h3: ({ children, ...props }) => {
                    const id = nextHeadingId();
                    return <h3 id={id} data-toc-id={id} {...props}>{children}</h3>;
                  },
                  a: ({ href, children, ...props }) => (
                    <a
                      {...props}
                      href={href}
                      target="_blank"
                      rel="noopener noreferrer"
                      onClick={(e) => {
                        e.preventDefault();
                        if (href) void openUrl(href);
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
                      return <code className={className} {...props}>{children}</code>;
                    }
                    return (
                      <MarkdownCodeBlock
                        code={content.replace(/\n$/, "")}
                        language={markdownCodeLanguage(className)}
                        isDark={t.isDark}
                      />
                    );
                  },
                }}
              >
                {fixMarkdown(md)}
              </ReactMarkdown>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ─── Progress Panel ───────────────────────────────────────────────────────

function ProgressPanel({
  accountId,
  progressMediaId,
  rounds,
  webCount,
  thinkingCount,
  entryCount,
}: {
  accountId: string;
  progressMediaId: string;
  rounds?: number;
  webCount?: number;
  thinkingCount?: number;
  entryCount?: number;
}) {
  const t = useTheme();
  const accent = t.isDark ? ACCENT_BLUE_DARK : ACCENT_BLUE;
  const railColor = t.isDark ? "rgba(255,255,255,0.12)" : "rgba(0,0,0,0.08)";
  const [entries, setEntries] = useState<ProgressEntry[] | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setEntries(null);
    setErr(null);
    (async () => {
      try {
        const txt = await invoke<string>("read_media_file", {
          accountId,
          mediaId: progressMediaId,
        });
        const parsed = JSON.parse(txt) as ProgressEntry[];
        if (!cancelled) setEntries(Array.isArray(parsed) ? parsed : []);
      } catch (e: any) {
        if (!cancelled) setErr(String(e?.message ?? e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [accountId, progressMediaId]);

  // 把条目按"遇到的 round"划分到各轮；非 thinking 条目继承上一个 thinking 的 round
  const grouped = useMemo(() => {
    if (!entries) return [];
    const out: { round: number | null; items: ProgressEntry[] }[] = [];
    let currentRound: number | null = null;
    for (const e of entries) {
      const r = e.type === "thinking" && typeof e.round === "number" ? e.round : currentRound;
      if (out.length === 0 || out[out.length - 1].round !== r) {
        out.push({ round: r, items: [] });
      }
      out[out.length - 1].items.push(e);
      if (e.type === "thinking" && typeof e.round === "number") currentRound = e.round;
    }
    return out;
  }, [entries]);

  const headerBits = formatProgressBits({
    rounds: rounds ?? 0,
    webCount: webCount ?? 0,
    thinkingCount: thinkingCount ?? 0,
    entryCount: entryCount ?? 0,
  });

  const hostOf = (url?: string): string => {
    if (!url) return "";
    try {
      return new URL(url).host.replace(/^www\./, "");
    } catch {
      return url;
    }
  };

  return (
    <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
      <div
        style={{
          padding: "10px 18px",
          borderBottom: `1px solid ${t.divider}`,
          fontSize: 12,
          color: t.textSub,
          flex: "0 0 auto",
        }}
      >
        {headerBits.join(" · ") || "research progress"}
      </div>
      <div style={{ flex: 1, overflowY: "auto", padding: "20px 18px 60px" }}>
        {entries === null && !err && (
          <div style={{ display: "flex", alignItems: "center", gap: 8, color: t.textMuted, fontSize: 13 }}>
            <SpinnerIcon color={t.textMuted} />
            正在加载调研过程…
          </div>
        )}
        {err && <div style={{ color: "#d84a3a", fontSize: 13 }}>加载失败：{err}</div>}
        {entries !== null && !err && (
          <div style={{ position: "relative" }}>
            {/* 整条贯通竖线 */}
            <div
              style={{
                position: "absolute",
                left: 7.5,
                top: 10,
                bottom: 10,
                width: 1,
                background: railColor,
              }}
            />
            {grouped.map((g, gi) => {
              // 把连续的 web_search 合并为一个 chunk
              type Chunk =
                | { kind: "item"; entry: ProgressEntry }
                | { kind: "webGroup"; list: ProgressEntry[] };
              const chunks: Chunk[] = [];
              let buf: ProgressEntry[] = [];
              const flush = () => {
                if (buf.length > 0) {
                  chunks.push({ kind: "webGroup", list: buf });
                  buf = [];
                }
              };
              for (const e of g.items) {
                if (e.type === "web_search") buf.push(e);
                else { flush(); chunks.push({ kind: "item", entry: e }); }
              }
              flush();

              return (
                <div key={gi} style={{ marginBottom: gi === grouped.length - 1 ? 0 : 18 }}>
                  {g.round !== null && (
                    <div
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: 10,
                        padding: "4px 0 10px",
                        position: "relative",
                        zIndex: 1,
                      }}
                    >
                      <div
                        style={{
                          width: 16,
                          height: 16,
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "center",
                          background: t.isDark ? "#1c1f25" : "#ffffff",
                          flexShrink: 0,
                        }}
                      >
                        <div
                          style={{
                            width: 7,
                            height: 7,
                            borderRadius: "50%",
                            background: t.isDark ? "rgba(124,167,255,0.35)" : "rgba(0,113,227,0.28)",
                          }}
                        />
                      </div>
                      <div
                        style={{
                          fontSize: 11,
                          fontWeight: 600,
                          letterSpacing: 0.4,
                          textTransform: "uppercase",
                          color: t.textMuted,
                        }}
                      >
                        第 {g.round + 1} 轮
                      </div>
                    </div>
                  )}
                  {chunks.map((c, i) =>
                    c.kind === "item" ? (
                      <ProgressItem key={i} entry={c.entry} accent={accent} />
                    ) : (
                      <WebGroupRow key={i} list={c.list} accent={accent} hostOf={hostOf} />
                    ),
                  )}
                </div>
              );
            })}
            {grouped.length === 0 && (
              <div style={{ color: t.textMuted, fontSize: 13 }}>暂无调研记录</div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function WebGroupRow({
  list,
  accent,
  hostOf,
}: {
  list: ProgressEntry[];
  accent: string;
  hostOf: (u?: string) => string;
}) {
  const t = useTheme();
  return (
    <div style={{ display: "flex", gap: 12, alignItems: "flex-start", padding: "6px 0" }}>
      {/* 共享 dot */}
      <div style={{ position: "relative", zIndex: 1, paddingTop: 10 }}>
        <div
          style={{
            width: 16,
            height: 16,
            borderRadius: "50%",
            background: t.isDark ? "#1c1f25" : "#ffffff",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            flexShrink: 0,
          }}
        >
          <div
            style={{
              width: 9,
              height: 9,
              borderRadius: "50%",
              border: `1.5px solid ${accent}`,
              background: t.isDark ? "#1c1f25" : "#ffffff",
            }}
          />
        </div>
      </div>
      {/* 小 chip 网格 */}
      <div
        style={{
          flex: 1,
          minWidth: 0,
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))",
          gap: 6,
          padding: "4px 0",
        }}
      >
        {list.map((e, i) => (
          <WebChip key={i} entry={e} accent={accent} host={hostOf(e.url)} />
        ))}
      </div>
    </div>
  );
}

function WebChip({
  entry,
  accent,
  host,
}: {
  entry: ProgressEntry;
  accent: string;
  host: string;
}) {
  const t = useTheme();
  const [hovered, setHovered] = useState(false);
  const clickable = !!entry.url;
  return (
    <button
      type="button"
      onClick={() => { if (clickable) void openUrl(entry.url!); }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      disabled={!clickable}
      title={entry.url}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        minWidth: 0,
        padding: "6px 10px",
        borderRadius: 8,
        border: `1px solid ${t.divider}`,
        background: hovered && clickable ? (t.isDark ? "rgba(124,167,255,0.08)" : "rgba(0,113,227,0.05)") : "transparent",
        cursor: clickable ? "pointer" : "default",
        textAlign: "left",
        color: t.text,
        transition: "background 0.15s, border-color 0.15s",
        borderColor: hovered && clickable ? (t.isDark ? "rgba(124,167,255,0.35)" : "rgba(0,113,227,0.3)") : t.divider,
      }}
    >
      <div
        style={{
          width: 20,
          height: 20,
          borderRadius: 5,
          background: t.isDark ? "rgba(124,167,255,0.12)" : "rgba(0,113,227,0.08)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          flex: "0 0 auto",
        }}
      >
        <GlobeIcon color={accent} size={11} />
      </div>
      <div style={{ minWidth: 0, flex: 1 }}>
        <div
          style={{
            fontSize: 12,
            fontWeight: 600,
            color: t.text,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            lineHeight: 1.3,
          }}
        >
          {entry.page_title || host || entry.url}
        </div>
        {host && (entry.page_title || entry.url !== host) && (
          <div
            style={{
              fontSize: 10.5,
              color: t.textMuted,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              lineHeight: 1.3,
            }}
          >
            {host}
          </div>
        )}
      </div>
    </button>
  );
}

function ProgressItem({
  entry,
  accent,
}: {
  entry: ProgressEntry;
  accent: string;
}) {
  const t = useTheme();
  const isThinking = entry.type === "thinking";

  const dot = (
    <div style={{ position: "relative", zIndex: 1, paddingTop: 6 }}>
      <div
        style={{
          width: 16,
          height: 16,
          borderRadius: "50%",
          background: t.isDark ? "#1c1f25" : "#ffffff",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          flexShrink: 0,
        }}
      >
        {isThinking ? (
          <div
            style={{
              width: 12,
              height: 12,
              borderRadius: "50%",
              background: t.isDark ? "rgba(124,167,255,0.20)" : "rgba(0,113,227,0.18)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <div style={{ width: 6, height: 6, borderRadius: "50%", background: accent }} />
          </div>
        ) : (
          <div
            style={{
              width: 9,
              height: 9,
              borderRadius: "50%",
              border: `1.5px solid ${accent}`,
              background: t.isDark ? "#1c1f25" : "#ffffff",
            }}
          />
        )}
      </div>
    </div>
  );

  return (
    <div style={{ display: "flex", gap: 10, alignItems: "flex-start", padding: "6px 0" }}>
      {dot}
      <div style={{ flex: 1, minWidth: 0, padding: isThinking ? "4px 0" : "8px 0", borderRadius: 10 }}>
        {isThinking ? (
          <>
            {entry.title && (
              <div style={{ fontSize: 13, fontWeight: 600, color: t.text, lineHeight: 1.45 }}>
                {entry.title}
              </div>
            )}
            {entry.description && (
              <div
                style={{
                  marginTop: entry.title ? 4 : 0,
                  fontSize: 12.5,
                  color: t.textSub,
                  lineHeight: 1.65,
                  whiteSpace: "pre-wrap",
                  wordBreak: "break-word",
                }}
              >
                {entry.description}
              </div>
            )}
          </>
        ) : (
          <div style={{ display: "flex", alignItems: "center", gap: 10, minWidth: 0 }}>
            <div
              style={{
                width: 26,
                height: 26,
                borderRadius: 7,
                background: t.isDark ? "rgba(124,167,255,0.12)" : "rgba(0,113,227,0.08)",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                flex: "0 0 auto",
              }}
            >
              <DocIcon color={accent} size={13} />
            </div>
            <div
              style={{
                fontSize: 13,
                fontWeight: 600,
                color: t.text,
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
                minWidth: 0,
                flex: 1,
              }}
            >
              {entry.filename || "文件"}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

