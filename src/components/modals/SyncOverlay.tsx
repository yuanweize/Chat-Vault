import { useTranslation } from "react-i18next";
import { useTheme } from "../../theme";
import { SpinnerIcon } from "../Icons";

export function SyncOverlay({ importingAccountData, preparingExportData }: { importingAccountData: boolean; preparingExportData: boolean }) {
  const theme = useTheme();
  const { t } = useTranslation();
  const message = importingAccountData ? t("app.importing") : preparingExportData ? t("app.readingData") : t("app.exporting");
  return <div className="modal-backdrop" style={{ zIndex: 5000 }} role="status" aria-live="polite"><div style={{ minWidth: 260, padding: "26px 30px", border: `1px solid ${theme.border}`, borderRadius: 16, display: "flex", flexDirection: "column", alignItems: "center", color: theme.text, background: theme.cardBg, boxShadow: "var(--shadow-md)" }}><SpinnerIcon size={26} color="var(--accent)" /><div style={{ marginTop: 14, fontSize: 13, fontWeight: 650 }}>{message}</div></div></div>;
}
