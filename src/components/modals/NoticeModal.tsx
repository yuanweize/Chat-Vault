import { useTranslation } from "react-i18next";
import { useTheme } from "../../theme";
import { InfoIcon } from "../Icons";

export function NoticeModal({ title, lines, onClose }: { title: string; lines: string[]; onClose: () => void }) {
  const theme = useTheme();
  const { t } = useTranslation();
  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="notice-title" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <div className="modal-panel" style={{ background: theme.cardBg, borderColor: theme.border }}>
        <div className="modal-body" style={{ padding: 22 }}>
          <div style={{ width: 42, height: 42, borderRadius: 12, display: "grid", placeItems: "center", color: "var(--accent)", background: "var(--accent-soft)" }}><InfoIcon size={20} /></div>
          <h2 id="notice-title" className="modal-title" style={{ marginTop: 15, color: theme.text }}>{title}</h2>
          <div style={{ marginTop: 9, color: theme.textSub, fontSize: 12.5, lineHeight: 1.65 }}>
            {lines.map((line, index) => <div key={`${index}-${line}`} style={{ overflowWrap: "anywhere" }}>{line}</div>)}
          </div>
        </div>
        <footer className="modal-footer" style={{ borderColor: theme.divider }}><button className="button-primary" onClick={onClose}>{t("common.ok")}</button></footer>
      </div>
    </div>
  );
}
