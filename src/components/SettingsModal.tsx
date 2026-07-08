import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { useTheme } from "../theme";
import { hoverHandlers } from "../utils/hoverHandlers";
import { exportAllToZip } from "../utils/exportUtils";

interface AppSettings {
  syncInterval: string | { custom: number };
  syncOnStartup: boolean;
  showSyncNotification: boolean;
  syncAccountIds: string[];
  autoSyncEnabled: boolean;
  customDataDirectory: string;
  defaultExportFormat: string;
  runInBackground: boolean;
  hideDockIcon: boolean;
  startOnLogin: boolean;
  passwordHash: string;
  autoLockPolicy: string;
  theme: string;
  language: string;
  sidebarWidth: number;
}

interface SettingsModalProps {
  onClose: () => void;
}

export function SettingsModal({ onClose, accountId }: SettingsModalProps & { accountId?: string }) {
  const theme = useTheme();
  const { t, i18n } = useTranslation();
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [saving, setSaving] = useState(false);
  const [activeTab, setActiveTab] = useState("sync");

  // Load settings on mount
  useEffect(() => {
    invoke<AppSettings>("load_settings")
      .then(setSettings)
      .catch(err => console.error("Failed to load settings", err));
  }, []);

  const handleSave = async () => {
    if (!settings) return;
    setSaving(true);
    try {
      await invoke("save_settings", { settings });
      onClose();
    } catch (err) {
      console.error("Failed to save settings", err);
      alert("保存失败: " + err);
    } finally {
      setSaving(false);
    }
  };

  const updateSetting = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    setSettings(prev => prev ? { ...prev, [key]: value } : null);
  };

  if (!settings) return null;

  return (
    <div style={{
      position: "fixed",
      top: 0, left: 0, right: 0, bottom: 0,
      background: "rgba(0, 0, 0, 0.7)",
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      zIndex: 1000,
    }}>
      <div style={{
        width: 700,
        height: 500,
        background: theme.sidebarBg,
        borderRadius: 8,
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        border: `2px solid ${theme.border}`,
      }}>
        {/* Header */}
        <div style={{
          padding: "16px 20px",
          borderBottom: `1px solid ${theme.border}`,
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          background: theme.topBarBg,
        }}>
          <h2 style={{ margin: 0, fontSize: 18, color: theme.text, fontWeight: 700 }}>{t('settings.title')}</h2>
          <button onClick={onClose} style={{
            background: "none", border: "none", color: theme.textSub, fontSize: 20, cursor: "pointer"
          }}>&times;</button>
        </div>

        {/* Body */}
        <div style={{ display: "flex", flex: 1, overflow: "hidden" }}>
          {/* Sidebar */}
          <div style={{
            width: 160,
            borderRight: `1px solid ${theme.border}`,
            background: theme.topBarBg,
            display: "flex",
            flexDirection: "column",
            padding: "10px 0",
          }}>
            {[
              { id: "sync", label: t('settings.sync') },
              { id: "storage", label: t('settings.data') },
              { id: "run", label: t('settings.app') },
              { id: "security", label: t('settings.security') },
              { id: "appearance", label: "🎨 外观/Appearance" },
              { id: "about", label: t('settings.about') },
            ].map(tab => (
              <div
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                style={{
                  padding: "10px 20px",
                  cursor: "pointer",
                  color: activeTab === tab.id ? theme.text : theme.textSub,
                  background: activeTab === tab.id ? theme.hover : "transparent",
                  fontWeight: activeTab === tab.id ? 600 : 400,
                }}
                {...hoverHandlers(theme.hover)}
              >
                {tab.label}
              </div>
            ))}
          </div>

          {/* Content */}
          <div style={{ flex: 1, padding: 24, overflowY: "auto", color: theme.text }}>
            
            {activeTab === "sync" && (
              <div style={sectionStyle}>
                <h3 style={{fontWeight: 700}}>{t('settings.syncSettings', '同步设置')}</h3>
                <label style={rowStyle}>
                  <span style={{fontWeight: 600}}>{t('settings.autoSync', '启用自动同步')}</span>
                  <input type="checkbox" checked={settings.autoSyncEnabled} onChange={e => updateSetting("autoSyncEnabled", e.target.checked)} />
                </label>
                <label style={rowStyle}>
                  <span style={{fontWeight: 600}}>{t('settings.syncOnStartup', '启动时自动同步')}</span>
                  <input type="checkbox" checked={settings.syncOnStartup} onChange={e => updateSetting("syncOnStartup", e.target.checked)} />
                </label>
                <label style={rowStyle}>
                  <span style={{fontWeight: 600}}>{t('settings.syncInterval', '同步间隔')}</span>
                  <select 
                    value={typeof settings.syncInterval === 'string' ? settings.syncInterval : 'custom'} 
                    onChange={e => updateSetting("syncInterval", e.target.value)}
                    style={inputStyle(t)}
                  >
                    <option value="minutes30">30 分钟</option>
                    <option value="hour1">1 小时</option>
                    <option value="hours3">3 小时</option>
                    <option value="hours6">6 小时</option>
                    <option value="hours12">12 小时</option>
                    <option value="hours24">24 小时</option>
                  </select>
                </label>
                <label style={rowStyle}>
                  <span style={{fontWeight: 600}}>{t('settings.showSyncNotification', '显示同步完成通知')}</span>
                  <input type="checkbox" checked={settings.showSyncNotification} onChange={e => updateSetting("showSyncNotification", e.target.checked)} />
                </label>
              </div>
            )}

            {activeTab === "storage" && (
              <div style={sectionStyle}>
                <h3 style={{fontWeight: 700}}>{t('settings.storageSettings', '存储设置')}</h3>
                <label style={rowStyle}>
                  <span style={{fontWeight: 600}}>{t('settings.customDataDir', '自定义存储路径')}</span>
                  <div style={{display: 'flex', gap: 8}}>
                    <input 
                      type="text" 
                      value={settings.customDataDirectory} 
                      onChange={e => updateSetting("customDataDirectory", e.target.value)}
                      placeholder="留空则使用默认路径"
                      style={{...inputStyle(t), width: 200}}
                    />
                  </div>
                </label>
                <label style={rowStyle}>
                  <span style={{fontWeight: 600}}>{t('settings.defaultExportFormat', '默认导出格式')}</span>
                  <select 
                    value={settings.defaultExportFormat} 
                    onChange={e => updateSetting("defaultExportFormat", e.target.value)}
                    style={inputStyle(t)}
                  >
                    <option value="markdown">Markdown (.md)</option>
                    <option value="pdf">PDF (.pdf)</option>
                    <option value="json">结构化 JSON</option>
                    <option value="jsonl">原始 JSONL</option>
                  </select>
                </label>
                <div style={{marginTop: 16, padding: 12, background: theme.hover, borderRadius: 8}}>
                  <p style={{margin: "0 0 10px 0", fontSize: 13, fontWeight: 500}}>{t('settings.exportDirDesc', '本地会话 Markdown 及资源导出目录：')}</p>
                  <button onClick={async () => {
                    if (accountId) {
                      await exportAllToZip(accountId);
                    } else {
                      alert("请先登录账号");
                    }
                  }} style={{
                    padding: "6px 12px", background: theme.btnHoverBg, color: theme.text, border: "none", borderRadius: 6, cursor: "pointer", width: "100%"
                  }}>
                    {t('settings.openExportDir', '打开导出目录')}
                  </button>
                </div>
              </div>
            )}

            {activeTab === "run" && (
              <div style={sectionStyle}>
                <h3 style={{fontWeight: 700}}>{t('settings.runSettings', '运行设置')}</h3>
                <label style={rowStyle}>
                  <span style={{fontWeight: 600}}>{t('settings.runInBackground', '关闭窗口时最小化到托盘 (后台运行)')}</span>
                  <input type="checkbox" checked={settings.runInBackground} onChange={e => updateSetting("runInBackground", e.target.checked)} />
                </label>
                <label style={rowStyle}>
                  <span style={{fontWeight: 600}}>{t('settings.hideDockIcon', '隐藏 Dock 图标 (macOS 需重启应用)')}</span>
                  <input type="checkbox" checked={settings.hideDockIcon} onChange={e => updateSetting("hideDockIcon", e.target.checked)} />
                </label>
              </div>
            )}

            {activeTab === "security" && (
              <div style={sectionStyle}>
                <h3 style={{fontWeight: 700}}>{t('settings.securitySettings', '安全与隐私')}</h3>
                <label style={rowStyle}>
                  <span style={{fontWeight: 600}}>{t('settings.autoLock', '自动锁定')}</span>
                  <select 
                    value={settings.autoLockPolicy} 
                    onChange={e => updateSetting("autoLockPolicy", e.target.value)}
                    style={inputStyle(t)}
                  >
                    <option value="never">从不</option>
                    <option value="immediately">立即</option>
                    <option value="minutes1">1 分钟</option>
                    <option value="minutes5">5 分钟</option>
                    <option value="minutes15">15 分钟</option>
                    <option value="minutes30">30 分钟</option>
                  </select>
                </label>
                <div style={{marginTop: 16, padding: 12, background: theme.hover, borderRadius: 8}}>
                  <p style={{margin: "0 0 10px 0", fontSize: 13}}>目前密码状态: {settings.passwordHash ? "✅ 已设置" : "❌ 未设置"}</p>
                  <button onClick={() => {
                    // TODO: Implement password change dialog
                    alert("密码修改功能将在后续更新");
                  }} style={{
                    padding: "6px 12px", background: theme.btnHoverBg, color: theme.text, border: "none", borderRadius: 6, cursor: "pointer"
                  }}>
                    修改密码...
                  </button>
                </div>
              </div>
            )}

            {activeTab === "appearance" && (
              <div style={sectionStyle}>
                <h3 style={{fontWeight: 700}}>{t('settings.appearanceSettings', '外观设置')}</h3>
                <label style={rowStyle}>
                  <span style={{fontWeight: 600}}>{t('settings.themeMode', '主题模式')}</span>
                  <select 
                    value={settings.theme} 
                    onChange={e => updateSetting("theme", e.target.value)}
                    style={inputStyle(t)}
                  >
                    <option value="auto">跟随系统</option>
                    <option value="light">亮色</option>
                    <option value="dark">暗色</option>
                  </select>
                </label>
                <label style={rowStyle}>
                  <span style={{fontWeight: 600}}>{t('settings.language', '显示语言')}</span>
                  <select 
                    value={settings.language || "zh"} 
                    onChange={e => {
                      updateSetting("language", e.target.value);
                      i18n.changeLanguage(e.target.value);
                    }}
                    style={inputStyle(theme)}
                  >
                    <option value="zh">简体中文</option>
                    <option value="en">English</option>
                  </select>
                </label>
                <label style={rowStyle}>
                  <span style={{fontWeight: 600}}>{t('settings.sidebarWidth', '侧边栏宽度')}</span>
                  <input 
                    type="range" 
                    min={200} max={400} 
                    value={settings.sidebarWidth} 
                    onChange={e => updateSetting("sidebarWidth", parseInt(e.target.value, 10))} 
                  />
                  <span>{settings.sidebarWidth}px</span>
                </label>
              </div>
            )}

            {activeTab === "about" && (
              <div style={{textAlign: "center", paddingTop: 40}}>
                <h2 style={{marginBottom: 4}}>Chat Vault</h2>
                <p style={{color: theme.textSub, marginTop: 0}}>Version 3.0.0</p>
                <p style={{fontSize: 13}}>Based on gemini-collector by FirenzeLor</p>
              </div>
            )}

          </div>
        </div>

        {/* Footer */}
        <div style={{
          padding: "16px 24px",
          borderTop: `1px solid ${theme.border}`,
          display: "flex",
          justifyContent: "flex-end",
          gap: 12,
          background: theme.topBarBg,
        }}>
          <button 
            onClick={onClose}
            style={{
              padding: "8px 16px",
              background: "transparent",
              color: theme.text,
              border: `1px solid ${theme.border}`,
              borderRadius: 6,
              cursor: "pointer",
            }}
          >
            {t('settings.cancel', '取消')}
          </button>
          <button 
            onClick={handleSave}
            disabled={saving}
            style={{
              padding: "8px 24px",
              background: "#3b82f6",
              color: "white",
              border: "none",
              borderRadius: 6,
              cursor: saving ? "wait" : "pointer",
              fontWeight: 500,
            }}
          >
            {saving ? t('settings.saving', '保存中...') : t('settings.save', '保存设置')}
          </button>
        </div>
      </div>
    </div>
  );
}

const sectionStyle: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 16,
};

const rowStyle: React.CSSProperties = {
  display: "flex",
  justifyContent: "space-between",
  alignItems: "center",
  fontSize: 14,
};

const inputStyle = (t: any): React.CSSProperties => ({
  padding: "6px 10px",
  borderRadius: 4,
  border: `2px solid ${t.border}`,
  background: t.sidebarBg,
  color: t.text,
  outline: "none",
});
