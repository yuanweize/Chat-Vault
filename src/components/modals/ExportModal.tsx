import { useTheme } from "../../theme";
import { AccountExportStats, ConversationSummary } from "../../data/types";
import { formatBytes } from "../../utils/exportUtils"; // wait, I'll need to define formatBytes

export interface ExportModalProps {
  exportStats: AccountExportStats;
  exportTimeRange: "all" | "3d" | "7d" | "30d";
  setExportTimeRange: (val: "all" | "3d" | "7d" | "30d") => void;
  exportFormat: "zip" | "kelivo" | "kelivo-split";
  setExportFormat: (val: "zip" | "kelivo" | "kelivo-split") => void;
  conversationSummaries: ConversationSummary[];
  exportRangeBytesCache: Map<string, number>;
  exportRangeBytesLoading: boolean;
  exportingAccountData: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

export function ExportModal({
  exportStats,
  exportTimeRange,
  setExportTimeRange,
  exportFormat,
  setExportFormat,
  conversationSummaries,
  exportRangeBytesCache,
  exportRangeBytesLoading,
  exportingAccountData,
  onCancel,
  onConfirm,
}: ExportModalProps) {
  const theme = useTheme();

  const filteredSummaries =
    exportTimeRange === "all"
      ? null
      : (() => {
          const days =
            exportTimeRange === "3d" ? 3 : exportTimeRange === "7d" ? 7 : 30;
          const afterDate = new Date(Date.now() - days * 86400_000).toISOString();
          return conversationSummaries.filter((c) => c.updatedAt >= afterDate);
        })();

  const displayConvCount = filteredSummaries
    ? filteredSummaries.length
    : exportStats.conversationCount;
  const displayMediaCount = filteredSummaries
    ? filteredSummaries.reduce(
        (sum, c) => sum + (c.imageCount ?? 0) + (c.videoCount ?? 0),
        0
      )
    : exportStats.mediaFileCount;
  const cachedBytes = exportRangeBytesCache.get(exportTimeRange);
  const bytesText =
    cachedBytes !== undefined
      ? formatBytes(cachedBytes)
      : exportRangeBytesLoading
      ? "加载中…"
      : "—";

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,0.4)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 1000,
      }}
    >
      <div
        style={{
          width: 460,
          borderRadius: 14,
          background: theme.isDark ? "#1c1f25" : "#ffffff",
          border: `1px solid ${theme.border}`,
          padding: 22,
        }}
      >
        <div style={{ fontSize: 15, fontWeight: 700, color: theme.text }}>
          导出账号数据
        </div>
        <div style={{ display: "flex", gap: 12, marginTop: 14 }}>
          {/* Time Range */}
          <div style={{ flex: 1, background: theme.hover, borderRadius: 8, padding: 12 }}>
            <div style={{ fontSize: 11, fontWeight: 600, color: theme.textMuted, marginBottom: 10, letterSpacing: 0.5 }}>
              时间范围
            </div>
            {(
              [
                ["all", "全部"],
                ["3d", "3 天"],
                ["7d", "7 天"],
                ["30d", "一个月"],
              ] as const
            ).map(([val, label]) => (
              <div
                key={val}
                onClick={() => setExportTimeRange(val)}
                style={{ display: "flex", alignItems: "center", gap: 10, padding: "4px 0", cursor: "pointer" }}
              >
                <div
                  style={{
                    width: 10,
                    height: 10,
                    borderRadius: 5,
                    background: exportTimeRange === val ? "#0071e3" : "transparent",
                    border: exportTimeRange === val ? "none" : `1.5px solid ${theme.textMuted}`,
                    flexShrink: 0,
                  }}
                />
                <span style={{ fontSize: 13, color: theme.text }}>{label}</span>
              </div>
            ))}
          </div>
          {/* Export Format */}
          <div style={{ flex: 1, background: theme.hover, borderRadius: 8, padding: 12 }}>
            <div style={{ fontSize: 11, fontWeight: 600, color: theme.textMuted, marginBottom: 10, letterSpacing: 0.5 }}>
              导出格式
            </div>
            {(
              [
                ["zip", "原始"],
                ["kelivo", "Kelivo"],
                ["kelivo-split", "Kelivo（分包）"],
              ] as const
            ).map(([val, label]) => (
              <div
                key={val}
                onClick={() => setExportFormat(val)}
                style={{ display: "flex", alignItems: "center", gap: 10, padding: "4px 0", cursor: "pointer" }}
              >
                <div
                  style={{
                    width: 10,
                    height: 10,
                    borderRadius: 5,
                    background: exportFormat === val ? "#0071e3" : "transparent",
                    border: exportFormat === val ? "none" : `1.5px solid ${theme.textMuted}`,
                    flexShrink: 0,
                  }}
                />
                <span style={{ fontSize: 13, color: theme.text }}>{label}</span>
              </div>
            ))}
          </div>
        </div>

        {/* Stats */}
        <div style={{ marginTop: 12, padding: "10px 12px", background: theme.hover, borderRadius: 8, fontSize: 12, color: theme.textSub, lineHeight: 1.8 }}>
          <div>对话数: <span style={{ color: theme.text, fontWeight: 500 }}>{displayConvCount}</span></div>
          <div>媒体文件（估算）: <span style={{ color: theme.text, fontWeight: 500 }}>{displayMediaCount}</span></div>
          {filteredSummaries === null && (
            <>
              <div>文件总数: <span style={{ color: theme.text, fontWeight: 500 }}>{exportStats.totalFileCount}</span></div>
              <div>预估压缩后: <span style={{ color: theme.text, fontWeight: 500 }}>{formatBytes(exportStats.estimatedZipBytes)}</span></div>
            </>
          )}
          <div>媒体体积: <span style={{ color: theme.text, fontWeight: 500 }}>{bytesText}</span></div>
        </div>

        {/* Buttons */}
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 14 }}>
          <button
            onClick={onCancel}
            style={{ padding: "7px 16px", borderRadius: 8, border: "none", background: theme.btnHoverBg, color: theme.text, fontSize: 13, cursor: "pointer" }}
          >
            取消
          </button>
          <button
            onClick={onConfirm}
            disabled={exportingAccountData}
            style={{
              padding: "7px 16px",
              borderRadius: 8,
              border: "none",
              background: "#0071e3",
              color: "#fff",
              fontSize: 13,
              fontWeight: 600,
              cursor: exportingAccountData ? "default" : "pointer",
              opacity: exportingAccountData ? 0.6 : 1,
            }}
          >
            开始导出
          </button>
        </div>
      </div>
    </div>
  );
}
