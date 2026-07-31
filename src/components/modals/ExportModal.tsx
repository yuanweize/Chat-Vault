import { useTranslation } from "react-i18next";
import type { AccountExportStats, ConversationSummary } from "../../data/types";
import { useTheme } from "../../theme";
import { formatBytes } from "../../utils/exportUtils";
import { CloseIcon, ExportIcon } from "../Icons";

export interface ExportModalProps {
  exportStats: AccountExportStats;
  exportTimeRange: "all" | "3d" | "7d" | "30d";
  setExportTimeRange: (value: "all" | "3d" | "7d" | "30d") => void;
  exportFormat: "zip" | "kelivo" | "kelivo-split";
  setExportFormat: (value: "zip" | "kelivo" | "kelivo-split") => void;
  conversationSummaries: ConversationSummary[];
  exportRangeBytesCache: Map<string, number>;
  exportRangeBytesLoading: boolean;
  exportingAccountData: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

export function ExportModal({ exportStats, exportTimeRange, setExportTimeRange, exportFormat, setExportFormat, conversationSummaries, exportRangeBytesCache, exportRangeBytesLoading, exportingAccountData, onCancel, onConfirm }: ExportModalProps) {
  const theme = useTheme();
  const { t } = useTranslation();
  const filtered = exportTimeRange === "all" ? null : conversationSummaries.filter((conversation) => {
    const days = exportTimeRange === "3d" ? 3 : exportTimeRange === "7d" ? 7 : 30;
    return conversation.updatedAt >= new Date(Date.now() - days * 86_400_000).toISOString();
  });
  const conversationCount = filtered?.length ?? exportStats.conversationCount;
  const mediaCount = filtered?.reduce((total, conversation) => total + (conversation.imageCount ?? 0) + (conversation.videoCount ?? 0), 0) ?? exportStats.mediaFileCount;
  const bytes = exportRangeBytesCache.get(exportTimeRange);
  const bytesText = bytes === undefined ? (exportRangeBytesLoading ? t("common.loading") : "—") : formatBytes(bytes);

  const timeRanges = [
    ["all", t("exportModal.all")], ["3d", t("exportModal.days3")], ["7d", t("exportModal.days7")], ["30d", t("exportModal.days30")],
  ] as const;
  const formats = [
    ["zip", t("exportModal.original"), t("exportModal.originalDesc")],
    ["kelivo", t("exportModal.kelivo"), t("exportModal.kelivoDesc")],
    ["kelivo-split", t("exportModal.kelivoSplit"), t("exportModal.kelivoSplitDesc")],
  ] as const;

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="export-title" onMouseDown={(event) => { if (event.target === event.currentTarget && !exportingAccountData) onCancel(); }}>
      <div className="modal-panel" style={{ background: theme.cardBg, borderColor: theme.border }}>
        <header className="modal-header" style={{ borderColor: theme.divider }}>
          <div><h2 id="export-title" className="modal-title" style={{ color: theme.text }}>{t("exportModal.title")}</h2><div className="modal-subtitle" style={{ color: theme.textSub }}>{t("exportModal.subtitle")}</div></div>
          <button className="icon-button" onClick={onCancel} aria-label={t("common.close")} style={{ color: theme.textSub }}><CloseIcon size={18} /></button>
        </header>
        <div className="modal-body">
          {exportFormat !== "zip" && <fieldset style={{ margin: 0, padding: 0, border: 0 }}>
            <legend style={legendStyle(theme.textMuted)}>{t("exportModal.timeRange")}</legend>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: 7 }}>
              {timeRanges.map(([value, label]) => <ChoiceChip key={value} checked={exportTimeRange === value} label={label} name="export-range" onChange={() => setExportTimeRange(value)} />)}
            </div>
          </fieldset>}
          <fieldset style={{ margin: exportFormat === "zip" ? 0 : "20px 0 0", padding: 0, border: 0 }}>
            <legend style={legendStyle(theme.textMuted)}>{t("exportModal.format")}</legend>
            <div style={{ display: "grid", gap: 8 }}>
              {formats.map(([value, label, description]) => (
                <label key={value} style={{ padding: "11px 12px", border: `1px solid ${exportFormat === value ? "var(--accent)" : theme.border}`, borderRadius: 11, display: "flex", alignItems: "center", gap: 11, background: exportFormat === value ? "var(--accent-soft)" : theme.sidebarBg, cursor: "pointer" }}>
                  <input type="radio" name="export-format" value={value} checked={exportFormat === value} onChange={() => { setExportFormat(value); if (value === "zip") setExportTimeRange("all"); }} style={{ accentColor: "var(--accent)" }} />
                  <span><span style={{ display: "block", color: exportFormat === value ? "var(--accent)" : theme.text, fontSize: 13, fontWeight: 700 }}>{label}</span><span style={{ display: "block", marginTop: 2, color: theme.textMuted, fontSize: 11 }}>{description}</span></span>
                </label>
              ))}
            </div>
          </fieldset>
          <div style={{ marginTop: 18, padding: 14, borderRadius: 12, display: "grid", gridTemplateColumns: "1fr 1fr", gap: "10px 18px", background: theme.sidebarBg }}>
            <Stat label={t("exportModal.conversations")} value={String(conversationCount)} />
            <Stat label={t("exportModal.mediaFiles")} value={String(mediaCount)} />
            {!filtered && <><Stat label={t("exportModal.totalFiles")} value={String(exportStats.totalFileCount)} /><Stat label={t("exportModal.estimatedArchive")} value={formatBytes(exportStats.estimatedZipBytes)} /></>}
            <Stat label={t("exportModal.mediaSize")} value={bytesText} />
          </div>
        </div>
        <footer className="modal-footer" style={{ borderColor: theme.divider }}>
          <button className="button-secondary" onClick={onCancel} style={{ color: theme.text, borderColor: theme.border, background: "transparent" }}>{t("common.cancel")}</button>
          <button className="button-primary" onClick={onConfirm} disabled={exportingAccountData}><ExportIcon spinning={exportingAccountData} size={15} />{t("exportModal.start")}</button>
        </footer>
      </div>
    </div>
  );
}

function ChoiceChip({ checked, label, name, onChange }: { checked: boolean; label: string; name: string; onChange: () => void }) {
  const theme = useTheme();
  return <label style={{ minHeight: 38, border: `1px solid ${checked ? "var(--accent)" : theme.border}`, borderRadius: 10, display: "grid", placeItems: "center", color: checked ? "var(--accent)" : theme.textSub, background: checked ? "var(--accent-soft)" : theme.sidebarBg, cursor: "pointer", fontSize: 12, fontWeight: 650 }}><input type="radio" name={name} checked={checked} onChange={onChange} style={{ position: "absolute", opacity: 0, pointerEvents: "none" }} />{label}</label>;
}

function Stat({ label, value }: { label: string; value: string }) {
  const theme = useTheme();
  return <div><div style={{ color: theme.textMuted, fontSize: 10.5 }}>{label}</div><div style={{ marginTop: 2, color: theme.text, fontSize: 13, fontWeight: 700 }}>{value}</div></div>;
}

function legendStyle(color: string): React.CSSProperties {
  return { marginBottom: 9, color, fontSize: 10.5, fontWeight: 750, letterSpacing: ".06em", textTransform: "uppercase" };
}
