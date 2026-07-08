import { useTranslation } from "react-i18next";
import { useTheme } from "../../theme";

export interface ClearConfirmModalProps {
  accountName: string;
  onCancel: () => void;
  onConfirm: () => void;
}

export function ClearConfirmModal({ accountName, onCancel, onConfirm }: ClearConfirmModalProps) {
  const { t } = useTranslation();
  const theme = useTheme();
  const clearDialogBg = theme.isDark ? "#171b22" : "#ffffff";
  const clearDialogBorder = theme.isDark ? "rgba(255,255,255,0.14)" : "rgba(15,23,42,0.14)";

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 9999,
        background: "rgba(0,0,0,0.32)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      <div
        style={{
          width: 380,
          maxWidth: "calc(100vw - 32px)",
          borderRadius: 4,
          background: clearDialogBg,
          border: `2px solid ${clearDialogBorder}`,
          padding: 16,
        }}
      >
        <div style={{ fontSize: 15, fontWeight: 700, color: theme.text, marginBottom: 8 }}>
          {t("app.confirmClearTitle", "确认清空本地数据？")}
        </div>
        <div style={{ fontSize: 13, color: theme.textSub, lineHeight: 1.5, marginBottom: 14 }}>
          {t("app.confirmClearDesc", {
            name: accountName,
            defaultValue: `账号「${accountName}」的会话与媒体缓存将被删除，且不可恢复。`,
          })}
        </div>
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
          <button
            onClick={onCancel}
            style={{
              border: `1px solid ${clearDialogBorder}`,
              background: "transparent",
              color: theme.text,
              borderRadius: 8,
              padding: "7px 12px",
              fontSize: 12,
              cursor: "pointer",
            }}
          >
            {t("settings.cancel", "取消")}
          </button>
          <button
            onClick={onConfirm}
            style={{
              border: "none",
              background: "#d34b4b",
              color: "#fff",
              borderRadius: 8,
              padding: "7px 12px",
              fontSize: 12,
              fontWeight: 600,
              cursor: "pointer",
            }}
          >
            {t("app.confirmClear", "确认清空")}
          </button>
        </div>
      </div>
    </div>
  );
}
