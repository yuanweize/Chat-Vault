import { useTranslation } from "react-i18next";
import { useTheme } from "../../theme";

export interface SyncOverlayProps {
  importingAccountData: boolean;
  preparingExportData: boolean;
}

export function SyncOverlay({ importingAccountData, preparingExportData }: SyncOverlayProps) {
  const { t } = useTranslation();
  const theme = useTheme();

  return (
    <>
      <style>{`@keyframes spin { to { transform: rotate(360deg); } }`}</style>
      <div
        style={{
          position: "fixed",
          inset: 0,
          zIndex: 2000,
          background: "rgba(0,0,0,0.45)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <div
          style={{
            borderRadius: 14,
            padding: "28px 36px",
            background: theme.isDark ? "#1c1f25" : "#fff",
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
          }}
        >
          <div
            style={{
              width: 32,
              height: 32,
              border: "3px solid rgba(0,113,227,0.2)",
              borderTop: "3px solid #0071e3",
              borderRadius: "50%",
              animation: "spin 0.8s linear infinite",
            }}
          />
          <div style={{ marginTop: 14, fontSize: 14, color: theme.text }}>
            {importingAccountData
              ? t("app.importing", "导入中，请勿关闭…")
              : preparingExportData
              ? t("app.readingData", "正在读取数据…")
              : t("app.exporting", "导出中，请勿关闭…")}
          </div>
        </div>
      </div>
    </>
  );
}
