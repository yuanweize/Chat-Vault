import { useTranslation } from "react-i18next";
import { useTheme } from "../../theme";
import { TrashIcon } from "../Icons";

export function ClearConfirmModal({ accountName, onCancel, onConfirm }: { accountName: string; onCancel: () => void; onConfirm: () => void }) {
  const theme = useTheme();
  const { t } = useTranslation();
  return (
    <div className="modal-backdrop" role="alertdialog" aria-modal="true" aria-labelledby="clear-title" onMouseDown={(event) => { if (event.target === event.currentTarget) onCancel(); }}>
      <div className="modal-panel" style={{ background: theme.cardBg, borderColor: theme.border }}>
        <div className="modal-body" style={{ padding: 22 }}>
          <div style={{ width: 42, height: 42, borderRadius: 12, display: "grid", placeItems: "center", color: "var(--danger)", background: "color-mix(in srgb, var(--danger) 11%, transparent)" }}><TrashIcon size={20} /></div>
          <h2 id="clear-title" className="modal-title" style={{ marginTop: 15, color: theme.text }}>{t("app.confirmClearTitle")}</h2>
          <p style={{ margin: "8px 0 0", color: theme.textSub, fontSize: 13, lineHeight: 1.6 }}>{t("app.confirmClearDesc", { name: accountName })}</p>
        </div>
        <footer className="modal-footer" style={{ borderColor: theme.divider }}>
          <button className="button-secondary" onClick={onCancel} style={{ color: theme.text, borderColor: theme.border, background: "transparent" }}>{t("common.cancel")}</button>
          <button className="button-danger" onClick={onConfirm}>{t("app.confirmClear")}</button>
        </footer>
      </div>
    </div>
  );
}
