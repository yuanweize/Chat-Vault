import { useTranslation } from "react-i18next";
import type { Account } from "../data/types";
import { useTheme } from "../theme";
import { IS_WINDOWS } from "../utils/platform";
import { ChevronRightIcon, MoonIcon, SettingsIcon, SpinnerIcon, SunIcon, SyncIcon } from "./Icons";

interface AccountPickerProps {
  accounts: Account[];
  loading: boolean;
  importError?: string | null;
  onSelect: (account: Account) => void;
  isDark: boolean;
  onToggleDark: () => void;
  onReload?: () => void;
  reloading?: boolean;
  onOpenSettings?: () => void;
}

export function AccountPicker({ accounts, loading, importError, onSelect, isDark, onToggleDark, onReload, reloading = false, onOpenSettings }: AccountPickerProps) {
  const theme = useTheme();
  const { t } = useTranslation();
  const busy = loading || reloading;

  return (
    <div className="account-picker-shell" style={{ background: theme.appBg }}>
      <div data-tauri-drag-region style={{ position: "absolute", inset: "0 0 auto", height: IS_WINDOWS ? 8 : 52, zIndex: 2 }} />
      <div style={{ position: "absolute", top: 14, right: 14, zIndex: 3, display: "flex", gap: 4 }}>
        {onOpenSettings && <button className="icon-button" onClick={onOpenSettings} aria-label={t("settings.title")} title={t("settings.title")} style={{ color: theme.textSub }}><SettingsIcon size={17} /></button>}
        <button className="icon-button" onClick={onToggleDark} aria-label={isDark ? t("account.switchLight") : t("account.switchDark")} title={isDark ? t("account.switchLight") : t("account.switchDark")} style={{ color: theme.textSub }}>
          {isDark ? <SunIcon size={17} /> : <MoonIcon size={17} />}
        </button>
      </div>

      <main className="account-picker-content">
        <div className="brand-lockup">
          <img className="brand-icon" src="/app-icon.png?v=3.1.0" alt="" />
          <div className="brand-name" style={{ color: theme.text }}>Chat Vault</div>
          <div className="brand-tagline" style={{ color: theme.textSub }}>{t("account.selectAccountDesc")}</div>
        </div>

        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", margin: "0 2px 9px" }}>
          <h1 style={{ margin: 0, color: theme.text, fontSize: 14, fontWeight: 720 }}>{t("account.selectAccount")}</h1>
          {onReload && (
            <button className="icon-button" onClick={onReload} disabled={busy} aria-label={t("account.redetect")} title={t("account.redetect")} style={{ width: 30, height: 30, color: theme.textSub }}>
              <SyncIcon spinning={reloading} size={16} />
            </button>
          )}
        </div>

        <section className="account-card" aria-busy={busy} style={{ background: theme.cardBg, borderColor: theme.border }}>
          {busy ? (
            <div style={{ minHeight: 118, display: "grid", placeItems: "center", color: theme.textMuted }}><SpinnerIcon size={22} /></div>
          ) : accounts.length === 0 ? (
            <div style={{ padding: "30px 26px", textAlign: "center" }}>
              <div style={{ color: theme.text, fontSize: 14, fontWeight: 700 }}>{t("account.noLocalAccount")}</div>
              <p style={{ margin: "8px 0 0", color: theme.textSub, fontSize: 12.5, lineHeight: 1.6 }}>
                {IS_WINDOWS ? t("account.loginGoogle") : <>{t("account.autoTriedBrowser")}<br />{t("account.confirmGeminiLogin")}</>}
              </p>
              {IS_WINDOWS && onReload && <button className="button-primary" onClick={onReload} style={{ marginTop: 18 }}>{t("account.loginGoogleBtn")}</button>}
            </div>
          ) : accounts.map((account) => (
            <button className="account-row" key={account.id} onClick={() => onSelect(account)} style={{ color: theme.text, borderColor: theme.divider }}>
              <span className="avatar" style={{ width: 42, height: 42, background: account.avatarColor }}>{account.avatarText}</span>
              <span style={{ minWidth: 0 }}>
                <span style={{ display: "flex", alignItems: "center", gap: 7 }}>
                  <span style={{ overflow: "hidden", fontSize: 13.5, fontWeight: 700, textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{account.name}</span>
                  {account.listSyncPending && <span className="status-dot" title={t("account.syncIncomplete")} />}
                </span>
                <span style={{ display: "block", marginTop: 3, overflow: "hidden", color: theme.textSub, fontSize: 11.5, textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{account.email}</span>
              </span>
              <span style={{ textAlign: "right", whiteSpace: "nowrap" }}>
                <span style={{ display: "block", color: theme.textSub, fontSize: 11.5, fontWeight: 650 }}>{t("account.conversations", { count: account.conversationCount })}</span>
                <span style={{ display: "block", marginTop: 3, color: theme.textMuted, fontSize: 10.5 }}>{account.lastSyncAt ? account.lastSyncAt.slice(0, 10) : t("account.syncNotDone")}</span>
              </span>
              <ChevronRightIcon size={15} color={theme.textMuted} />
            </button>
          ))}
        </section>

        {importError && !busy && (
          <details style={{ marginTop: 12, padding: "11px 13px", border: `1px solid color-mix(in srgb, var(--danger) 35%, transparent)`, borderRadius: 12, color: "var(--danger)", background: "color-mix(in srgb, var(--danger) 7%, transparent)", fontSize: 11.5 }}>
            <summary style={{ cursor: "pointer", fontWeight: 700 }}>{t("account.diagnosticInfo")}</summary>
            <pre style={{ maxHeight: 160, margin: "9px 0 0", overflow: "auto", whiteSpace: "pre-wrap", wordBreak: "break-word", fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", fontSize: 10.5, lineHeight: 1.5 }}>{importError}</pre>
          </details>
        )}
      </main>
    </div>
  );
}
