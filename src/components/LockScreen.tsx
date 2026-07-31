import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { useTheme } from "../theme";
import { LockIcon } from "./Icons";

export function LockScreen({ onUnlock }: { onUnlock: () => void }) {
  const theme = useTheme();
  const { t } = useTranslation();
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [unlocking, setUnlocking] = useState(false);

  const handleUnlock = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!password || unlocking) return;
    setUnlocking(true);
    setError("");
    try {
      if (await invoke<boolean>("verify_unlock", { password })) onUnlock();
      else {
        setError(t("lock.wrongPassword"));
        setPassword("");
      }
    } catch (reason) {
      setError(String(reason));
    } finally {
      setUnlocking(false);
    }
  };

  return (
    <main className="lock-shell" style={{ background: theme.appBg }}>
      <section className="lock-card" aria-labelledby="lock-title">
        <div className="lock-card-inner" style={{ padding: 30, background: theme.cardBg, borderColor: theme.border, textAlign: "center" }}>
          <div style={{ width: 58, height: 58, margin: "0 auto 18px", borderRadius: 16, display: "grid", placeItems: "center", color: "var(--accent)", background: "var(--accent-soft)" }}><LockIcon size={25} /></div>
          <h1 id="lock-title" style={{ margin: 0, color: theme.text, fontSize: 20, fontWeight: 760, letterSpacing: "-.02em" }}>{t("lock.title")}</h1>
          <p style={{ margin: "7px 0 22px", color: theme.textSub, fontSize: 12.5 }}>{t("lock.subtitle")}</p>
          <form onSubmit={(event) => void handleUnlock(event)} style={{ display: "grid", gap: 12 }}>
            <input className="field-input" type="password" autoComplete="current-password" autoFocus value={password} onChange={(event) => setPassword(event.target.value)} placeholder={t("lock.enterPassword")} aria-invalid={!!error} style={{ width: "100%", minHeight: 42, color: theme.text, background: theme.sidebarBg, borderColor: error ? "var(--danger)" : theme.border }} />
            {error && <div role="alert" style={{ color: "var(--danger)", fontSize: 12, fontWeight: 650 }}>{error}</div>}
            <button className="button-primary" type="submit" disabled={unlocking || !password} style={{ minHeight: 42 }}>{unlocking ? t("lock.unlocking") : t("lock.unlock")}</button>
          </form>
        </div>
      </section>
    </main>
  );
}
