import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Conversation, ConversationSummary } from "../data/types";
import { useTheme } from "../theme";
import { TOP_BAR_HEIGHT } from "../utils/platform";
import { formatDateTime } from "../utils/dateTime";
import { hoverHandlers } from "../utils/hoverHandlers";
import { SidebarIcon, MoonIcon, SunIcon, ExternalLinkIcon, LogoutIcon, TrashIcon, SettingsIcon, ExportIcon } from "./Icons";
import { exportConversationToZip } from "../utils/exportUtils";
import { useTranslation } from "react-i18next";

interface TopBarProps {
  selectedConversation: Conversation | null;
  selectedSummary?: ConversationSummary | null;
  sidebarCollapsed: boolean;
  onToggleSidebar: () => void;
  isDark: boolean;
  onToggleDark: () => void;
  disableLogout?: boolean;
  onLogout: () => void;
  onClearConversation?: () => void;
  onOpenSettings?: () => void;
  accountId: string;
  authuser?: string | null;
}

export function TopBar({
  selectedConversation,
  selectedSummary = null,
  sidebarCollapsed,
  onToggleSidebar,
  isDark,
  onToggleDark,
  disableLogout = false,
  onLogout,
  onClearConversation,
  onOpenSettings,
  accountId,
  authuser = null,
}: TopBarProps) {
  const tTheme = useTheme();
  const { t } = useTranslation();
  const [exporting, setExporting] = useState(false);
  const imageCount = Math.max(0, selectedSummary?.imageCount ?? 0);
  const videoCount = Math.max(0, selectedSummary?.videoCount ?? 0);
  const createdAt = selectedConversation?.createdAt || selectedSummary?.updatedAt || "";

  return (
    <div
      id="topbar-root"
      data-tauri-drag-region
      style={{
        height: TOP_BAR_HEIGHT,
        flexShrink: 0,
        display: "flex",
        alignItems: "center",
        paddingLeft: 12,
        paddingRight: 12,
        position: "relative",
        background: tTheme.topBarBg,
        borderBottom: `2px solid ${tTheme.border}`,
      }}
    >
      {/* Toggle sidebar button */}
      <button
        onClick={onToggleSidebar}
        title={sidebarCollapsed ? t("topbar.expandSidebar") : t("topbar.collapseSidebar")}
        style={{
          ...iconBtn(),
          marginLeft: sidebarCollapsed ? 68 : 0,
          transition: "background 0.15s, margin-left 0.25s cubic-bezier(0.4,0,0.2,1)",
        }}
        {...hoverHandlers(tTheme.btnHoverBg)}
      >
        <SidebarIcon collapsed={sidebarCollapsed} color={tTheme.textSub} />
      </button>

      {/* Title */}
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", marginLeft: 12 }}>
        {selectedConversation ? (
          <>
            <div style={{ fontWeight: 700, fontSize: 14, color: tTheme.text, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {selectedConversation.title}
            </div>
            <div style={{ fontSize: 12, fontWeight: 500, color: tTheme.textSub, marginTop: 2, display: "flex", gap: 8 }}>
              <span>{formatDateTime(createdAt)}</span>
              <span>·</span>
              <span>{selectedSummary?.messageCount || selectedConversation.messages.length} msgs</span>
              {imageCount > 0 && <span>· 🖼️ {imageCount}</span>}
              {videoCount > 0 && <span>· 🎬 {videoCount}</span>}
            </div>
          </>
        ) : (
          <div style={{ fontWeight: 700, fontSize: 14, color: tTheme.textSub }}>
            {t("sidebar.search")}
          </div>
        )}
      </div>

      <div style={{ marginLeft: "auto", display: "flex", alignItems: "center", gap: 4 }}>
        {selectedConversation && (
          <button
            onClick={() => onClearConversation?.()}
            title={t("topbar.delete")}
            style={iconBtn()}
            {...hoverHandlers(tTheme.btnHoverBg)}
          >
            <TrashIcon color={tTheme.textSub} />
          </button>
        )}
        {selectedConversation && (
          <button
            onClick={async () => {
              if (exporting) return;
              setExporting(true);
              try {
                const success = await exportConversationToZip(selectedConversation, accountId);
                if (success) {
                  console.log("Exported successfully");
                }
              } catch (err: any) {
                alert(`${t("settings.exportFailed")}: ${err.message}`);
              } finally {
                setExporting(false);
              }
            }}
            title={exporting ? t("topbar.exporting") : t("topbar.export")}
            style={{...iconBtn(), opacity: exporting ? 0.5 : 1}}
            {...hoverHandlers(tTheme.btnHoverBg)}
          >
            <ExportIcon color={tTheme.textSub} />
          </button>
        )}
        {selectedConversation && (
          <button
            onClick={() => {
              window.print();
            }}
            title={t("topbar.exportPdf")}
            style={iconBtn()}
            {...hoverHandlers(tTheme.btnHoverBg)}
          >
            <span style={{ fontSize: 13, color: tTheme.textSub, fontWeight: 700 }}>PDF</span>
          </button>
        )}
        {selectedConversation && (
          <button
            onClick={() => {
              const bareId = selectedConversation.id.replace(/^c_/, "");
              const au = authuser ?? "0";
              void openUrl(`https://gemini.google.com/u/${au}/app/${bareId}`);
            }}
            title={t("topbar.openInGemini")}
            style={iconBtn()}
            {...hoverHandlers(tTheme.btnHoverBg)}
          >
            <ExternalLinkIcon color={tTheme.textSub} />
          </button>
        )}
        <button
          onClick={onToggleDark}
          style={iconBtn()}
          {...hoverHandlers(tTheme.btnHoverBg)}
          title={isDark ? t("account.switchLight") : t("account.switchDark")}
        >
          {isDark ? <SunIcon color={tTheme.textSub} /> : <MoonIcon color={tTheme.textSub} />}
        </button>

        <button
          onClick={() => onOpenSettings?.()}
          title={t("settings.title")}
          style={iconBtn()}
          {...hoverHandlers(tTheme.btnHoverBg)}
        >
          <SettingsIcon color={tTheme.textSub} />
        </button>

        <button
          onClick={() => {
            if (disableLogout) return;
            onLogout();
          }}
          title={t("account.selectAccount")}
          style={{ ...iconBtn(), opacity: disableLogout ? 0.55 : 1, cursor: disableLogout ? "default" : "pointer" }}
          {...(!disableLogout ? hoverHandlers(tTheme.btnHoverBg) : {})}
        >
          <LogoutIcon color={tTheme.textSub} />
        </button>
      </div>
    </div>
  );
}

function iconBtn(): React.CSSProperties {
  return {
    width: 28,
    height: 28,
    borderRadius: 4,
    border: "none",
    background: "transparent",
    cursor: "pointer",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    transition: "background 0.15s",
  };
}
