import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import type { Conversation, ConversationSummary } from "../data/types";
import { useTheme } from "../theme";
import { exportConversationToZip } from "../utils/exportUtils";
import { formatDateTime } from "../utils/dateTime";
import { TOP_BAR_HEIGHT } from "../utils/platform";
import {
  ExportIcon,
  ExternalLinkIcon,
  ImageIcon,
  LogoutIcon,
  MoonIcon,
  PdfIcon,
  SettingsIcon,
  SidebarIcon,
  SunIcon,
  TrashIcon,
  VideoIcon,
} from "./Icons";

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

export function TopBar({ selectedConversation, selectedSummary = null, sidebarCollapsed, onToggleSidebar, isDark, onToggleDark, disableLogout = false, onLogout, onClearConversation, onOpenSettings, accountId, authuser = null }: TopBarProps) {
  const theme = useTheme();
  const { t } = useTranslation();
  const [exporting, setExporting] = useState(false);
  const messageCount = selectedSummary?.messageCount ?? selectedConversation?.messages.length ?? 0;
  const imageCount = Math.max(0, selectedSummary?.imageCount ?? 0);
  const videoCount = Math.max(0, selectedSummary?.videoCount ?? 0);
  const createdAt = selectedConversation?.createdAt || selectedSummary?.updatedAt || "";

  const exportConversation = async () => {
    if (!selectedConversation || exporting) return;
    setExporting(true);
    try {
      await exportConversationToZip(selectedConversation, accountId);
    } catch (error) {
      window.alert(`${t("app.exportFailed")}: ${String(error)}`);
    } finally {
      setExporting(false);
    }
  };

  return (
    <header id="topbar-root" className="topbar" data-tauri-drag-region style={{ height: TOP_BAR_HEIGHT, background: theme.topBarBg, borderColor: theme.divider }}>
      <button className="icon-button" onClick={onToggleSidebar} title={t(sidebarCollapsed ? "topbar.expandSidebar" : "topbar.collapseSidebar")} aria-label={t(sidebarCollapsed ? "topbar.expandSidebar" : "topbar.collapseSidebar")} style={{ marginLeft: sidebarCollapsed ? 64 : 0, color: theme.textSub, transition: "margin-left 220ms ease, background 140ms ease" }}>
        <SidebarIcon collapsed={sidebarCollapsed} size={17} />
      </button>

      <div style={{ minWidth: 0, flex: 1 }}>
        <div style={{ overflow: "hidden", color: selectedConversation ? theme.text : theme.textSub, fontSize: 14, fontWeight: 720, textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {selectedConversation?.title ?? t("topbar.noSelection")}
        </div>
        {selectedConversation && (
          <div className="topbar-meta" style={{ marginTop: 3, color: theme.textMuted, fontSize: 11.5 }}>
            <span>{formatDateTime(createdAt)}</span>
            <span aria-hidden="true">·</span>
            <span>{t("topbar.messages", { count: messageCount })}</span>
            {imageCount > 0 && <span className="meta-pill"><ImageIcon size={12} />{t("topbar.images", { count: imageCount })}</span>}
            {videoCount > 0 && <span className="meta-pill"><VideoIcon size={12} />{t("topbar.videos", { count: videoCount })}</span>}
          </div>
        )}
      </div>

      <div className="topbar-actions">
        {selectedConversation && <>
          <TopBarButton label={t("topbar.delete")} onClick={() => onClearConversation?.()} color="var(--danger)"><TrashIcon size={16} /></TopBarButton>
          <TopBarButton label={exporting ? t("topbar.exporting") : t("topbar.export")} onClick={() => void exportConversation()} disabled={exporting}><ExportIcon size={16} spinning={exporting} /></TopBarButton>
          <TopBarButton label={t("topbar.exportPdf")} onClick={() => window.print()}><PdfIcon size={16} /></TopBarButton>
          <TopBarButton label={t("topbar.openInGemini")} onClick={() => { const id = selectedConversation.id.replace(/^c_/, ""); void openUrl(`https://gemini.google.com/u/${authuser ?? "0"}/app/${id}`); }}><ExternalLinkIcon size={16} /></TopBarButton>
          <span className="topbar-separator" style={{ background: theme.divider }} />
        </>}
        <TopBarButton label={t(isDark ? "account.switchLight" : "account.switchDark")} onClick={onToggleDark}>{isDark ? <SunIcon size={16} /> : <MoonIcon size={16} />}</TopBarButton>
        <TopBarButton label={t("settings.title")} onClick={() => onOpenSettings?.()}><SettingsIcon size={16} /></TopBarButton>
        <TopBarButton label={t("account.selectAccount")} onClick={onLogout} disabled={disableLogout}><LogoutIcon size={16} /></TopBarButton>
      </div>
    </header>
  );
}

function TopBarButton({ label, onClick, disabled = false, color, children }: { label: string; onClick: () => void; disabled?: boolean; color?: string; children: React.ReactNode }) {
  const theme = useTheme();
  return <button className="icon-button" onClick={onClick} disabled={disabled} title={label} aria-label={label} style={{ color: color ?? theme.textSub }}>{children}</button>;
}
