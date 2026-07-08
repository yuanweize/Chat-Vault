import React from "react";
import { Account } from "../data/types";
import { useTheme } from "../theme";
import { IS_WINDOWS } from "../utils/platform";
import { hoverHandlers } from "../utils/hoverHandlers";
import { SpinnerIcon, SunIcon, MoonIcon, SyncIcon } from "./Icons";
import { useTranslation } from "react-i18next";

interface AccountPickerProps {
  accounts: Account[];
  loading: boolean;
  importError?: string | null;
  onSelect: (account: Account) => void;
  isDark: boolean;
  onToggleDark: () => void;
  onReload?: () => void;
  reloading?: boolean;
}

export function AccountPicker({ accounts, loading, importError, onSelect, isDark, onToggleDark, onReload, reloading }: AccountPickerProps) {
  const tTheme = useTheme();
  const { t } = useTranslation();

  return (
    <div style={{ width: "100%", height: "100vh", background: tTheme.appBg, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", position: "relative" }}>
      {/* Drag region for window */}
      <div data-tauri-drag-region style={{ position: "absolute", top: 0, left: 0, right: 0, height: IS_WINDOWS ? 8 : 52 }} />

      {/* Dark mode toggle */}
      <button
        onClick={onToggleDark}
        title={isDark ? t("account.switchLight") : t("account.switchDark")}
        style={{ position: "absolute", top: 14, right: 14, width: 28, height: 28, borderRadius: 4, border: `2px solid ${tTheme.border}`, background: tTheme.topBarBg, cursor: "pointer", display: "flex", alignItems: "center", justifyContent: "center", transition: "background 0.15s" }}
        {...hoverHandlers(tTheme.btnHoverBg)}
      >
        {isDark ? <SunIcon color={tTheme.textSub} /> : <MoonIcon color={tTheme.textSub} />}
      </button>

      {/* App identity */}
      <div style={{ textAlign: "center", marginBottom: 36 }}>
        <div style={{ width: 56, height: 56, borderRadius: 8, background: "#3b82f6", display: "flex", alignItems: "center", justifyContent: "center", margin: "0 auto 14px" }}>
          <svg width="28" height="28" viewBox="0 0 24 24" fill="white"><path d="M12 2l2.4 7.4H22l-6.2 4.5 2.4 7.4L12 17l-6.2 4.3 2.4-7.4L2 9.4h7.6z" /></svg>
        </div>
        <div style={{ fontSize: 22, fontWeight: 800, color: tTheme.text, letterSpacing: -0.5 }}>Chat Vault</div>
        <div style={{ fontSize: 13, color: tTheme.textSub, marginTop: 4, fontWeight: 500 }}>{t("account.selectAccount")}</div>
      </div>

      {/* Reload icon — right above the card */}
      {onReload && (
        <button
          onClick={onReload}
          disabled={reloading || loading}
          title={t("account.redetect")}
          style={{ marginBottom: 6, background: "transparent", border: "none", cursor: reloading || loading ? "default" : "pointer", padding: 6, borderRadius: 4, display: "flex", alignItems: "center", justifyContent: "center", opacity: reloading || loading ? 0.4 : 0.7, transition: "opacity 0.15s" }}
          onMouseEnter={(e) => { if (!reloading && !loading) (e.currentTarget as HTMLElement).style.opacity = "1"; }}
          onMouseLeave={(e) => { if (!reloading && !loading) (e.currentTarget as HTMLElement).style.opacity = "0.7"; }}
        >
          <SyncIcon spinning={!!reloading} color={tTheme.textSub} />
        </button>
      )}

      {/* Content area */}
      <div style={{ width: 360, background: tTheme.cardBg, borderRadius: 8, border: `2px solid ${tTheme.border}`, overflow: "hidden", minHeight: 64 }}>
        {loading ? (
          /* Loading state */
          <div style={{ display: "flex", alignItems: "center", justifyContent: "center", padding: 32 }}>
            <SpinnerIcon color={tTheme.textMuted} />
          </div>
        ) : accounts.length === 0 ? (
          /* No accounts */
          <div style={{ display: "flex", flexDirection: "column", alignItems: "center", padding: "28px 24px" }}>
            <div style={{ fontSize: 13, color: tTheme.textSub, textAlign: "center", lineHeight: 1.6, fontWeight: 500 }}>
              {IS_WINDOWS ? (<>
                {t("account.noLocalAccount")}<br />
                {t("account.loginGoogle")}
              </>) : (<>
                {t("account.noLocalAccount")}<br />
                {t("account.autoTriedBrowser")}<br />
                {t("account.confirmGeminiLogin")}
              </>)}
            </div>
            {IS_WINDOWS && onReload && (
              <button
                onClick={onReload}
                disabled={reloading || loading}
                style={{
                  marginTop: 18,
                  padding: "10px 28px",
                  borderRadius: 4,
                  border: "none",
                  background: "#3b82f6",
                  color: "#fff",
                  fontSize: 14,
                  fontWeight: 600,
                  cursor: reloading || loading ? "default" : "pointer",
                  opacity: reloading || loading ? 0.6 : 1,
                  transition: "opacity 0.15s, background 0.15s",
                }}
              >
                {reloading ? t("account.waitingLogin") : t("account.loginGoogleBtn")}
              </button>
            )}
            {importError && (
              <div style={{
                marginTop: 16,
                width: "100%",
                padding: "12px 14px",
                borderRadius: 4,
                background: tTheme.isDark ? "#7f1d1d" : "#fee2e2",
                border: `2px solid ${tTheme.isDark ? "#b91c1c" : "#f87171"}`,
              }}>
                <div style={{ fontSize: 12, fontWeight: 700, color: tTheme.isDark ? "#fca5a5" : "#b91c1c", marginBottom: 6 }}>
                  {t("account.diagnosticInfo")}
                </div>
                <pre style={{
                  fontSize: 11,
                  lineHeight: 1.5,
                  color: tTheme.isDark ? "#fca5a5" : "#991b1b",
                  margin: 0,
                  whiteSpace: "pre-wrap",
                  wordBreak: "break-all",
                  maxHeight: 200,
                  overflowY: "auto",
                  fontFamily: "ui-monospace, 'SF Mono', Menlo, monospace",
                }}>
                  {importError}
                </pre>
              </div>
            )}
          </div>
        ) : (
          /* Account list */
          accounts.map((account, i) => (
            <AccountRow
              key={account.id}
              account={account}
              showDivider={i < accounts.length - 1}
              onClick={() => onSelect(account)}
            />
          ))
        )}
      </div>
    </div>
  );
}

function AccountRow({ account, showDivider, onClick }: { account: Account; showDivider: boolean; onClick: () => void }) {
  const tTheme = useTheme();
  const { t } = useTranslation();
  const [hovered, setHovered] = React.useState(false);

  return (
    <div
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{ padding: "13px 18px", display: "flex", alignItems: "center", gap: 12, cursor: "pointer", borderBottom: showDivider ? `2px solid ${tTheme.border}` : "none", background: hovered ? tTheme.hover : "transparent", transition: "background 0.12s" }}
    >
      <div style={{ width: 36, height: 36, borderRadius: 4, background: account.avatarColor, display: "flex", alignItems: "center", justifyContent: "center", color: "#fff", fontWeight: 700, fontSize: 15, flexShrink: 0 }}>
        {account.avatarText}
      </div>
      <div style={{ flex: 1, overflow: "hidden" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <div style={{ fontSize: 14, fontWeight: 700, color: tTheme.text, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{account.name}</div>
          {account.listSyncPending && (
            <span
              title={t("account.syncIncomplete")}
              style={{
                width: 8,
                height: 8,
                borderRadius: 4,
                background: "#ef4444",
                flexShrink: 0,
              }}
            />
          )}
        </div>
        <div style={{ fontSize: 12, fontWeight: 500, color: tTheme.textSub, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", marginTop: 1 }}>{account.email}</div>
      </div>
      <div style={{ textAlign: "right", flexShrink: 0 }}>
        <div style={{ fontSize: 12, fontWeight: 600, color: tTheme.textMuted }}>{t("account.conversations", { count: account.conversationCount })}</div>
        <div style={{ fontSize: 11, fontWeight: 500, color: tTheme.textMuted, marginTop: 1 }}>{account.lastSyncAt ? account.lastSyncAt.slice(0, 10) : t("account.syncNotDone")}</div>
      </div>
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke={tTheme.textMuted} strokeWidth="3" strokeLinecap="round" strokeLinejoin="round"><polyline points="9 18 15 12 9 6" /></svg>
    </div>
  );
}

