import React from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Conversation, ConversationSummary } from "../data/types";
import { useTheme } from "../theme";
import { TOP_BAR_HEIGHT } from "../utils/platform";
import { formatDateTime } from "../utils/dateTime";
import { hoverHandlers } from "../utils/hoverHandlers";
import { SidebarIcon, MoonIcon, SunIcon, ExternalLinkIcon, LogoutIcon, TrashIcon } from "./Icons";

interface TopBarProps {
  selectedConversation: Conversation | null;
  selectedSummary?: ConversationSummary | null;
  sidebarCollapsed: boolean;
  onToggleSidebar: () => void;
  isDark: boolean;
  onToggleDark: () => void;
  disableLogout?: boolean;
  onLogout: () => void;
  authuser?: string | null;
  onClearConversation?: () => void;
}

export function TopBar({
  selectedConversation,
  selectedSummary = null,
  sidebarCollapsed,
  onToggleSidebar,
  isDark, onToggleDark, disableLogout = false, onLogout,
  authuser = null, onClearConversation,
}: TopBarProps) {
  const t = useTheme();
  const imageCount = Math.max(0, selectedSummary?.imageCount ?? 0);
  const videoCount = Math.max(0, selectedSummary?.videoCount ?? 0);
  const createdAt = selectedConversation?.createdAt || selectedSummary?.updatedAt || "";
  const subtitleParts: string[] = [];
  if (imageCount > 0) subtitleParts.push(`图片 ${imageCount}`);
  if (videoCount > 0) subtitleParts.push(`视频 ${videoCount}`);
  subtitleParts.push(`创建于 ${formatDateTime(createdAt)}`);
  const subtitle = subtitleParts.join(" · ");

  return (
    <div
      data-tauri-drag-region
      style={{
        height: TOP_BAR_HEIGHT,
        flexShrink: 0,
        display: "flex",
        alignItems: "center",
        paddingLeft: 12,
        paddingRight: 12,
        position: "relative",
        background: t.topBarBg,
        backdropFilter: "blur(30px) saturate(112%)",
        WebkitBackdropFilter: "blur(30px) saturate(112%)",
      }}
    >
      {/* Toggle sidebar button */}
      <button
        onClick={onToggleSidebar}
        title={sidebarCollapsed ? "展开侧边栏" : "收起侧边栏"}
        style={{
          ...iconBtn(),
          marginLeft: sidebarCollapsed ? 68 : 0,
          transition: "background 0.15s, margin-left 0.25s cubic-bezier(0.4,0,0.2,1)",
        }}
        {...hoverHandlers(t.btnHoverBg)}
      >
        <SidebarIcon collapsed={sidebarCollapsed} color={t.textSub} />
      </button>

      {/* Title - centered and width-constrained to avoid overlapping controls */}
      {selectedConversation && (
        <div
          style={{
            position: "absolute",
            left: sidebarCollapsed ? 152 : 84,
            right: 96,
            top: "50%",
            transform: "translateY(-50%)",
            textAlign: "center",
            pointerEvents: "none",
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
          }}
        >
          <div
            style={{
              fontSize: 13,
              fontWeight: 600,
              color: t.text,
              width: "60%",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {selectedConversation.title}
          </div>
          <div
            style={{
              fontSize: 11,
              color: t.textSub,
              marginTop: 1,
              width: "60%",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {subtitle}
          </div>
        </div>
      )}

      {/* Right: open in browser + dark mode toggle + logout */}
      <div style={{ marginLeft: "auto", display: "flex", alignItems: "center", gap: 4 }}>
        {/* Clear current conversation detail */}
        {selectedConversation && (
          <button
            onClick={() => onClearConversation?.()}
            title="清除当前对话内容"
            style={iconBtn()}
            {...hoverHandlers(t.btnHoverBg)}
          >
            <TrashIcon color={t.textSub} />
          </button>
        )}
        {/* Open in Gemini */}
        {selectedConversation && (
          <button
            onClick={() => {
              const bareId = selectedConversation.id.replace(/^c_/, "");
              const au = authuser ?? "0";
              void openUrl(`https://gemini.google.com/u/${au}/app/${bareId}`);
            }}
            title="在浏览器中打开"
            style={iconBtn()}
            {...hoverHandlers(t.btnHoverBg)}
          >
            <ExternalLinkIcon color={t.textSub} />
          </button>
        )}
        {/* Dark mode toggle */}
        <button
          onClick={onToggleDark}
          title={isDark ? "切换到亮色模式" : "切换到暗色模式"}
          style={iconBtn()}
          {...hoverHandlers(t.btnHoverBg)}
        >
          {isDark ? <SunIcon color={t.textSub} /> : <MoonIcon color={t.textSub} />}
        </button>

        {/* Logout */}
        <button
          onClick={() => {
            if (disableLogout) return;
            onLogout();
          }}
          title="退出账号"
          style={{ ...iconBtn(), opacity: disableLogout ? 0.55 : 1, cursor: disableLogout ? "default" : "pointer" }}
          {...(!disableLogout ? hoverHandlers(t.btnHoverBg) : {})}
        >
          <LogoutIcon color={t.textSub} />
        </button>
      </div>
    </div>
  );
}

function iconBtn(): React.CSSProperties {
  return {
    width: 28,
    height: 28,
    borderRadius: 7,
    border: "none",
    background: "transparent",
    cursor: "pointer",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    flexShrink: 0,
    transition: "background 0.15s",
  };
}

