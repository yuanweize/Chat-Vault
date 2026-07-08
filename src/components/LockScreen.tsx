import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTheme } from "../theme";
import { useTranslation } from "react-i18next";

interface LockScreenProps {
  onUnlock: () => void;
}

export function LockScreen({ onUnlock }: LockScreenProps) {
  const tTheme = useTheme();
  const { t } = useTranslation();
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [unlocking, setUnlocking] = useState(false);

  const handleUnlock = async (e?: React.FormEvent) => {
    e?.preventDefault();
    if (!password) return;
    setUnlocking(true);
    setError("");
    try {
      const success = await invoke<boolean>("verify_unlock", { password });
      if (success) {
        onUnlock();
      } else {
        setError(t("lock.wrongPassword"));
        setPassword("");
      }
    } catch (err: any) {
      setError(err.toString());
    } finally {
      setUnlocking(false);
    }
  };

  return (
    <div style={{
      display: "flex",
      flexDirection: "column",
      alignItems: "center",
      justifyContent: "center",
      height: "100vh",
      background: tTheme.sidebarBg,
      color: tTheme.text,
      fontFamily: "system-ui, -apple-system, sans-serif"
    }}>
      <div style={{
        width: 340,
        padding: 32,
        borderRadius: 4,
        background: tTheme.topBarBg,
        border: `2px solid ${tTheme.border}`,
        display: "flex",
        flexDirection: "column",
        alignItems: "center"
      }}>
        <div style={{ fontSize: 48, marginBottom: 16 }}>🔒</div>
        <h2 style={{ margin: "0 0 24px 0", fontSize: 20, fontWeight: 700 }}>{t("lock.title")}</h2>
        
        <form onSubmit={handleUnlock} style={{ width: "100%", display: "flex", flexDirection: "column", gap: 16 }}>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder={t("lock.enterPassword")}
            autoFocus
            style={{
              padding: "12px 14px",
              borderRadius: 4,
              border: `2px solid ${error ? "#ef4444" : tTheme.border}`,
              background: tTheme.sidebarBg,
              color: tTheme.text,
              fontSize: 14,
              outline: "none",
              width: "100%",
              boxSizing: "border-box",
              transition: "border-color 0.15s"
            }}
          />
          {error && <div style={{ color: "#ef4444", fontSize: 13, textAlign: "center", fontWeight: 600 }}>{error}</div>}
          
          <button
            type="submit"
            disabled={unlocking || !password}
            style={{
              padding: "12px 16px",
              background: "#3b82f6",
              color: "white",
              border: "none",
              borderRadius: 4,
              fontSize: 14,
              fontWeight: 600,
              cursor: unlocking || !password ? "not-allowed" : "pointer",
              opacity: unlocking || !password ? 0.7 : 1,
              transition: "opacity 0.15s, background 0.15s"
            }}
          >
            {unlocking ? t("lock.unlocking") : t("lock.unlock")}
          </button>
        </form>
      </div>
    </div>
  );
}
