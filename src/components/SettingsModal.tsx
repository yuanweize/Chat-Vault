import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import type { AppSettings } from "../data/settings";
import { normalizeSettings } from "../data/settings";
import { useTheme } from "../theme";
import { openExportDirectory } from "../utils/exportUtils";
import {
  AppWindowIcon,
  AppearanceIcon,
  CloseIcon,
  InfoIcon,
  LockIcon,
  SettingsIcon,
  StorageIcon,
  SyncIcon,
} from "./Icons";

type TabId = "sync" | "storage" | "run" | "security" | "appearance" | "about";

interface SettingsModalProps {
  onClose: () => void;
  accountId?: string;
  initialSettings?: AppSettings | null;
  onSaved?: (settings: AppSettings) => void;
}

export function SettingsModal({ onClose, accountId, initialSettings, onSaved }: SettingsModalProps) {
  const theme = useTheme();
  const { t, i18n } = useTranslation();
  const [settings, setSettings] = useState<AppSettings | null>(initialSettings ?? null);
  const [saving, setSaving] = useState(false);
  const [activeTab, setActiveTab] = useState<TabId>("sync");
  const [loadError, setLoadError] = useState("");
  const [passwordForm, setPasswordForm] = useState({ current: "", next: "", confirm: "" });
  const [passwordBusy, setPasswordBusy] = useState(false);
  const [passwordMessage, setPasswordMessage] = useState<{ kind: "success" | "error"; text: string } | null>(null);

  useEffect(() => {
    if (initialSettings) {
      setSettings(normalizeSettings(initialSettings));
      return;
    }
    invoke<AppSettings>("load_settings")
      .then((value) => setSettings(normalizeSettings(value)))
      .catch((error) => setLoadError(String(error)));
  }, [initialSettings]);

  useEffect(() => {
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !saving && !passwordBusy) onClose();
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [onClose, passwordBusy, saving]);

  const navItems = useMemo(() => [
    { id: "sync" as const, label: t("settings.sync"), icon: SyncIcon },
    { id: "storage" as const, label: t("settings.data"), icon: StorageIcon },
    { id: "run" as const, label: t("settings.app"), icon: AppWindowIcon },
    { id: "security" as const, label: t("settings.security"), icon: LockIcon },
    { id: "appearance" as const, label: t("settings.appearance"), icon: AppearanceIcon },
    { id: "about" as const, label: t("settings.about"), icon: InfoIcon },
  ], [t]);

  const updateSetting = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    setSettings((previous) => previous ? { ...previous, [key]: value } : previous);
  };

  const handleSave = async () => {
    if (!settings || saving) return;
    setSaving(true);
    setLoadError("");
    try {
      const normalized = normalizeSettings(settings);
      await invoke("save_settings", { settings: normalized });
      onSaved?.(normalized);
      onClose();
    } catch (error) {
      setLoadError(`${t("settings.saveFailed")}: ${String(error)}`);
    } finally {
      setSaving(false);
    }
  };

  const refreshSettings = async () => {
    const refreshed = normalizeSettings(await invoke<AppSettings>("load_settings"));
    setSettings(refreshed);
    onSaved?.(refreshed);
  };

  const updatePassword = async (remove = false) => {
    if (!settings || passwordBusy) return;
    setPasswordMessage(null);
    if (!remove && passwordForm.next.length < 8) {
      setPasswordMessage({ kind: "error", text: t("settings.passwordTooShort") });
      return;
    }
    if (!remove && passwordForm.next !== passwordForm.confirm) {
      setPasswordMessage({ kind: "error", text: t("settings.passwordMismatch") });
      return;
    }
    setPasswordBusy(true);
    try {
      await invoke("set_password", {
        currentPassword: passwordForm.current,
        newPassword: remove ? "" : passwordForm.next,
      });
      await refreshSettings();
      setPasswordForm({ current: "", next: "", confirm: "" });
      setPasswordMessage({ kind: "success", text: t(remove ? "settings.passwordRemoved" : "settings.passwordUpdated") });
    } catch (error) {
      const raw = String(error);
      setPasswordMessage({
        kind: "error",
        text: raw.includes("CURRENT_PASSWORD_INVALID") ? t("settings.currentPasswordInvalid") : raw,
      });
    } finally {
      setPasswordBusy(false);
    }
  };

  const panelStyle = { background: theme.cardBg, borderColor: theme.border };
  const subtleStyle = { color: theme.textSub };
  const groupStyle = { background: theme.cardBg, borderColor: theme.border };
  const rowStyle = { borderColor: theme.divider };
  const fieldStyle = { color: theme.text, background: theme.sidebarBg, borderColor: theme.border };

  return (
    <div
      className="modal-backdrop"
      role="dialog"
      aria-modal="true"
      aria-labelledby="settings-title"
      onMouseDown={(event) => { if (event.target === event.currentTarget && !saving && !passwordBusy) onClose(); }}
    >
      <div className="modal-panel settings-panel" style={panelStyle}>
        <header className="modal-header" style={{ borderColor: theme.divider }}>
          <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
            <div style={{ width: 34, height: 34, borderRadius: 10, display: "grid", placeItems: "center", color: "var(--accent)", background: "var(--accent-soft)" }}>
              <SettingsIcon size={18} />
            </div>
            <div>
              <h2 id="settings-title" className="modal-title" style={{ color: theme.text }}>{t("settings.title")}</h2>
              <div className="modal-subtitle" style={subtleStyle}>{t("settings.subtitle")}</div>
            </div>
          </div>
          <button className="icon-button" onClick={onClose} aria-label={t("common.close")} style={{ color: theme.textSub }}>
            <CloseIcon size={18} />
          </button>
        </header>

        {!settings ? (
          <div style={{ flex: 1, display: "grid", placeItems: "center", color: loadError ? "var(--danger)" : theme.textSub, padding: 24 }}>
            {loadError || t("common.loading")}
          </div>
        ) : (
          <div className="settings-layout">
            <nav className="settings-nav" aria-label={t("settings.title")} style={{ borderColor: theme.divider, background: theme.sidebarBg }}>
              {navItems.map(({ id, label, icon: NavIcon }) => (
                <button
                  key={id}
                  className="settings-nav-button"
                  onClick={() => setActiveTab(id)}
                  aria-current={activeTab === id ? "page" : undefined}
                  style={{ color: activeTab === id ? "var(--accent)" : theme.textSub, background: activeTab === id ? "var(--accent-soft)" : "transparent" }}
                >
                  <NavIcon size={17} spinning={false} />
                  {label}
                </button>
              ))}
            </nav>

            <main className="settings-content">
              {activeTab === "sync" && (
                <SettingsSection title={t("settings.syncSettings")} description={t("settings.syncSettingsDesc")}>
                  <div className="setting-group" style={groupStyle}>
                    <SettingToggle label={t("settings.enableAutoSync")} description={t("settings.enableAutoSyncDesc")} checked={settings.autoSyncEnabled} onChange={(value) => updateSetting("autoSyncEnabled", value)} rowStyle={rowStyle} />
                    <SettingToggle label={t("settings.syncOnStartup")} description={t("settings.syncOnStartupDesc")} checked={settings.syncOnStartup} onChange={(value) => updateSetting("syncOnStartup", value)} rowStyle={rowStyle} />
                    <SettingRow label={t("settings.syncInterval")} rowStyle={rowStyle}>
                      <select className="field-select" value={typeof settings.syncInterval === "string" ? settings.syncInterval : "hours6"} onChange={(event) => updateSetting("syncInterval", event.target.value as AppSettings["syncInterval"])} style={fieldStyle}>
                        <option value="minutes30">{t("settings.interval30m")}</option>
                        <option value="hour1">{t("settings.interval1h")}</option>
                        <option value="hours3">{t("settings.interval3h")}</option>
                        <option value="hours6">{t("settings.interval6h")}</option>
                        <option value="hours12">{t("settings.interval12h")}</option>
                        <option value="hours24">{t("settings.interval24h")}</option>
                      </select>
                    </SettingRow>
                  </div>
                </SettingsSection>
              )}

              {activeTab === "storage" && (
                <SettingsSection title={t("settings.storageSettings")} description={t("settings.storageSettingsDesc")}>
                  <div style={{ padding: 16, border: `1px solid ${theme.border}`, borderRadius: 14, background: theme.sidebarBg }}>
                    <div className="setting-label">{t("settings.openExportFolder")}</div>
                    <div className="setting-description" style={subtleStyle}>{t("settings.openExportFolderDesc")}</div>
                    <button className="button-secondary" style={{ marginTop: 12, color: theme.text, borderColor: theme.border, background: theme.cardBg }} onClick={() => accountId ? void openExportDirectory(accountId) : setLoadError(t("settings.loginRequired"))}>
                      {t("settings.openExportFolder")}
                    </button>
                  </div>
                </SettingsSection>
              )}

              {activeTab === "run" && (
                <SettingsSection title={t("settings.runSettings")} description={t("settings.runSettingsDesc")}>
                  <div className="setting-group" style={groupStyle}>
                    <SettingToggle label={t("settings.minimizeToTray")} description={t("settings.minimizeToTrayDesc")} checked={settings.runInBackground} onChange={(value) => updateSetting("runInBackground", value)} rowStyle={rowStyle} />
                    <SettingToggle label={t("settings.hideDockIcon")} description={t("settings.hideDockIconDesc")} checked={settings.hideDockIcon} onChange={(value) => updateSetting("hideDockIcon", value)} rowStyle={rowStyle} />
                  </div>
                </SettingsSection>
              )}

              {activeTab === "security" && (
                <SettingsSection title={t("settings.securityAndPrivacy")} description={t("settings.securityDesc")}>
                  <div className="setting-group" style={groupStyle}>
                    <SettingRow label={t("settings.autoLock")} rowStyle={rowStyle}>
                      <select className="field-select" value={settings.autoLockPolicy} onChange={(event) => updateSetting("autoLockPolicy", event.target.value as AppSettings["autoLockPolicy"])} style={fieldStyle}>
                        <option value="never">{t("settings.lockNever")}</option>
                        <option value="immediately">{t("settings.lockImmediately")}</option>
                        <option value="minutes1">{t("settings.lock1Min")}</option>
                        <option value="minutes5">{t("settings.lock5Min")}</option>
                        <option value="minutes15">{t("settings.lock15Min")}</option>
                        <option value="minutes30">{t("settings.lock30Min")}</option>
                      </select>
                    </SettingRow>
                  </div>
                  <div style={{ marginTop: 16, padding: 16, border: `1px solid ${theme.border}`, borderRadius: 14, background: theme.sidebarBg }}>
                    <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12, marginBottom: 14 }}>
                      <span className="setting-label">{t("settings.passwordStatus")}</span>
                      <span style={{ color: settings.passwordHash ? "var(--success)" : theme.textMuted, fontSize: 12, fontWeight: 700 }}>{t(settings.passwordHash ? "settings.passwordSet" : "settings.passwordNotSet")}</span>
                    </div>
                    <div style={{ display: "grid", gap: 10 }}>
                      {settings.passwordHash && <input type="password" className="field-input" autoComplete="current-password" value={passwordForm.current} placeholder={t("settings.currentPassword")} onChange={(event) => setPasswordForm((value) => ({ ...value, current: event.target.value }))} style={{ ...fieldStyle, width: "100%" }} />}
                      <input type="password" className="field-input" autoComplete="new-password" value={passwordForm.next} placeholder={t("settings.newPassword")} onChange={(event) => setPasswordForm((value) => ({ ...value, next: event.target.value }))} style={{ ...fieldStyle, width: "100%" }} />
                      <input type="password" className="field-input" autoComplete="new-password" value={passwordForm.confirm} placeholder={t("settings.confirmPassword")} onChange={(event) => setPasswordForm((value) => ({ ...value, confirm: event.target.value }))} style={{ ...fieldStyle, width: "100%" }} />
                    </div>
                    {passwordMessage && <div style={{ marginTop: 10, color: passwordMessage.kind === "error" ? "var(--danger)" : "var(--success)", fontSize: 12, fontWeight: 650 }}>{passwordMessage.text}</div>}
                    <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 14 }}>
                      {settings.passwordHash && <button className="button-danger" disabled={passwordBusy || !passwordForm.current} onClick={() => void updatePassword(true)}>{t("settings.removePassword")}</button>}
                      <button className="button-primary" disabled={passwordBusy || !passwordForm.next || (settings.passwordHash ? !passwordForm.current : false)} onClick={() => void updatePassword(false)}>{t(settings.passwordHash ? "settings.changePassword" : "settings.setPassword")}</button>
                    </div>
                  </div>
                </SettingsSection>
              )}

              {activeTab === "appearance" && (
                <SettingsSection title={t("settings.appearanceSettings")} description={t("settings.appearanceDesc")}>
                  <div className="setting-group" style={groupStyle}>
                    <SettingRow label={t("settings.themeMode")} rowStyle={rowStyle}>
                      <select className="field-select" value={settings.theme} onChange={(event) => updateSetting("theme", event.target.value as AppSettings["theme"])} style={fieldStyle}>
                        <option value="auto">{t("settings.themeAuto")}</option>
                        <option value="light">{t("settings.themeLight")}</option>
                        <option value="dark">{t("settings.themeDark")}</option>
                      </select>
                    </SettingRow>
                    <SettingRow label={t("settings.language")} rowStyle={rowStyle}>
                      <select className="field-select" value={settings.language} onChange={(event) => { const language = event.target.value as AppSettings["language"]; updateSetting("language", language); void i18n.changeLanguage(language); }} style={fieldStyle}>
                        <option value="zh-CN">{t("settings.languageZh")}</option>
                        <option value="en">{t("settings.languageEn")}</option>
                      </select>
                    </SettingRow>
                    <SettingRow label={t("settings.sidebarWidth")} rowStyle={rowStyle}>
                      <div style={{ display: "flex", alignItems: "center", gap: 10, width: 220 }}>
                        <input type="range" min={240} max={380} step={4} value={settings.sidebarWidth} onChange={(event) => updateSetting("sidebarWidth", Number(event.target.value))} style={{ width: "100%", accentColor: "var(--accent)" }} />
                        <span style={{ width: 46, color: theme.textSub, fontSize: 12, textAlign: "right" }}>{settings.sidebarWidth}px</span>
                      </div>
                    </SettingRow>
                  </div>
                </SettingsSection>
              )}

              {activeTab === "about" && (
                <div style={{ minHeight: 360, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", textAlign: "center" }}>
                  <img src="/app-icon.png" width={84} height={84} alt="" style={{ borderRadius: 22, filter: "drop-shadow(0 14px 24px rgba(38,48,110,.2))" }} />
                  <h3 style={{ margin: "18px 0 4px", color: theme.text, fontSize: 23, letterSpacing: "-.03em" }}>Chat Vault</h3>
                  <p style={{ margin: 0, color: theme.textSub, fontSize: 13 }}>{t("settings.aboutTagline")}</p>
                  <div style={{ marginTop: 20, color: theme.textMuted, fontSize: 12 }}>{t("settings.version", { version: "3.1.0" })}</div>
                  <div style={{ marginTop: 5, maxWidth: 430, color: theme.textMuted, fontSize: 11.5, lineHeight: 1.5 }}>{t("settings.basedOn")}</div>
                </div>
              )}
            </main>
          </div>
        )}

        <footer className="modal-footer" style={{ borderColor: theme.divider }}>
          {loadError && <span style={{ marginRight: "auto", color: "var(--danger)", fontSize: 12 }}>{loadError}</span>}
          <button className="button-secondary" onClick={onClose} style={{ color: theme.text, borderColor: theme.border, background: "transparent" }}>{t("common.cancel")}</button>
          <button className="button-primary" onClick={() => void handleSave()} disabled={!settings || saving}>{saving ? t("common.saving") : t("common.save")}</button>
        </footer>
      </div>
    </div>
  );
}

function SettingsSection({ title, description, children }: { title: string; description: string; children: React.ReactNode }) {
  const theme = useTheme();
  return <section><h3 className="settings-section-title" style={{ color: theme.text }}>{title}</h3><p className="settings-section-description" style={{ color: theme.textSub }}>{description}</p>{children}</section>;
}

function SettingRow({ label, description, children, rowStyle }: { label: string; description?: string; children: React.ReactNode; rowStyle?: React.CSSProperties }) {
  const theme = useTheme();
  return <div className="setting-row" style={rowStyle}><div><div className="setting-label" style={{ color: theme.text }}>{label}</div>{description && <div className="setting-description" style={{ color: theme.textMuted }}>{description}</div>}</div>{children}</div>;
}

function SettingToggle({ label, description, checked, onChange, rowStyle }: { label: string; description?: string; checked: boolean; onChange: (value: boolean) => void; rowStyle?: React.CSSProperties }) {
  return <SettingRow label={label} description={description} rowStyle={rowStyle}><label className="toggle"><input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} /><span className="toggle-track" /></label></SettingRow>;
}
