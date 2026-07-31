import { useTranslation } from "react-i18next";
import { useTheme } from "../../theme";
import { SpinnerIcon, StorageIcon } from "../Icons";

export interface LegacyMigrationInfo {
  available: boolean;
  accountCount: number;
  conversationCount: number;
  mediaFileCount: number;
  totalBytes: number;
}

interface LegacyMigrationModalProps {
  info: LegacyMigrationInfo;
  busy: boolean;
  error: string | null;
  onCancel: () => void;
  onConfirm: () => void;
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(unit === 0 || value >= 100 ? 0 : 1)} ${units[unit]}`;
}

export function LegacyMigrationModal({ info, busy, error, onCancel, onConfirm }: LegacyMigrationModalProps) {
  const theme = useTheme();
  const { t } = useTranslation();
  const stats = [
    [t("migration.accounts"), info.accountCount.toLocaleString()],
    [t("migration.conversations"), info.conversationCount.toLocaleString()],
    [t("migration.media"), info.mediaFileCount.toLocaleString()],
    [t("migration.size"), formatBytes(info.totalBytes)],
  ];

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="migration-title">
      <div className="modal-panel migration-modal" style={{ background: theme.cardBg, borderColor: theme.border }}>
        <div className="modal-body" style={{ padding: 24 }}>
          <div style={{ width: 44, height: 44, borderRadius: 13, display: "grid", placeItems: "center", color: "var(--accent)", background: "var(--accent-soft)" }}>
            <StorageIcon size={21} />
          </div>
          <h2 id="migration-title" className="modal-title" style={{ marginTop: 15, color: theme.text }}>{t("migration.title")}</h2>
          <p style={{ margin: "7px 0 0", color: theme.textSub, fontSize: 12.5, lineHeight: 1.65 }}>{t("migration.subtitle")}</p>

          <div className="migration-stats" style={{ marginTop: 18, borderColor: theme.border }}>
            {stats.map(([label, value]) => (
              <div key={label} className="migration-stat">
                <span style={{ color: theme.textMuted }}>{label}</span>
                <strong style={{ color: theme.text }}>{value}</strong>
              </div>
            ))}
          </div>

          <div style={{ marginTop: 14, padding: "12px 13px", borderRadius: 11, color: theme.textSub, background: theme.btnBg, fontSize: 11.5, lineHeight: 1.6 }}>
            {t("migration.compatibility")}
          </div>
          {error && <div style={{ marginTop: 12, color: "var(--danger)", fontSize: 11.5, lineHeight: 1.5 }}>{t("migration.failed", { error })}</div>}
        </div>
        <footer className="modal-footer" style={{ borderColor: theme.divider }}>
          <button className="button-secondary" onClick={onCancel} disabled={busy}>{t("migration.later")}</button>
          <button className="button-primary" onClick={onConfirm} disabled={busy}>
            {busy ? <><SpinnerIcon size={15} /> {t("migration.migrating")}</> : t("migration.confirm")}
          </button>
        </footer>
      </div>
    </div>
  );
}
