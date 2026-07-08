import { useTheme } from "../../theme";

export interface NoticeModalProps {
  title: string;
  lines: string[];
  onClose: () => void;
}

export function NoticeModal({ title, lines, onClose }: NoticeModalProps) {
  const theme = useTheme();
  const clearDialogBg = theme.isDark ? "#171b22" : "#ffffff";
  const clearDialogBorder = theme.isDark ? "rgba(255,255,255,0.14)" : "rgba(15,23,42,0.14)";

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 10001,
        background: "rgba(0,0,0,0.32)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      <div
        style={{
          width: 430,
          maxWidth: "calc(100vw - 32px)",
          borderRadius: 4,
          background: clearDialogBg,
          border: `2px solid ${clearDialogBorder}`,
          padding: 16,
        }}
      >
        <div style={{ fontSize: 15, fontWeight: 700, color: theme.text, marginBottom: 8 }}>
          {title}
        </div>
        <div style={{ fontSize: 12, color: theme.textSub, lineHeight: 1.6, marginBottom: 14 }}>
          {lines.map((line, idx) => (
            <div key={`${idx}_${line}`}>{line}</div>
          ))}
        </div>
        <div style={{ display: "flex", justifyContent: "flex-end" }}>
          <button
            onClick={onClose}
            style={{
              border: "none",
              background: "#0071e3",
              color: "#fff",
              borderRadius: 4,
              padding: "7px 12px",
              fontSize: 12,
              fontWeight: 600,
              cursor: "pointer",
            }}
          >
            OK
          </button>
        </div>
      </div>
    </div>
  );
}
