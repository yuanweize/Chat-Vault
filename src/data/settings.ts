export type SyncInterval =
  | "minutes30"
  | "hour1"
  | "hours3"
  | "hours6"
  | "hours12"
  | "hours24"
  | { custom: number };

export type ExportFormat =
  | "markdown"
  | "pdf"
  | "json"
  | "jsonl"
  | "kelivo"
  | "kelivoSplit";

export type AutoLockPolicy =
  | "immediately"
  | "minutes1"
  | "minutes5"
  | "minutes15"
  | "minutes30"
  | "never";

export type ThemePreference = "auto" | "light" | "dark";
export type LanguagePreference = "zh-CN" | "en";

export interface AppSettings {
  syncInterval: SyncInterval;
  syncOnStartup: boolean;
  showSyncNotification: boolean;
  syncAccountIds: string[];
  autoSyncEnabled: boolean;
  customDataDirectory: string;
  defaultExportFormat: ExportFormat;
  runInBackground: boolean;
  hideDockIcon: boolean;
  startOnLogin: boolean;
  passwordHash: string;
  autoLockPolicy: AutoLockPolicy;
  theme: ThemePreference;
  language: LanguagePreference;
  sidebarWidth: number;
}

export const DEFAULT_SETTINGS: AppSettings = {
  syncInterval: "hours6",
  syncOnStartup: false,
  showSyncNotification: true,
  syncAccountIds: [],
  autoSyncEnabled: true,
  customDataDirectory: "",
  defaultExportFormat: "markdown",
  runInBackground: true,
  hideDockIcon: false,
  startOnLogin: false,
  passwordHash: "",
  autoLockPolicy: "never",
  theme: "auto",
  language: "zh-CN",
  sidebarWidth: 280,
};

export function normalizeSettings(value: Partial<AppSettings> | null | undefined): AppSettings {
  const next = { ...DEFAULT_SETTINGS, ...(value ?? {}) };
  return {
    ...next,
    language: next.language === "en" ? "en" : "zh-CN",
    sidebarWidth: Math.min(380, Math.max(240, Number(next.sidebarWidth) || 280)),
    syncAccountIds: Array.isArray(next.syncAccountIds) ? next.syncAccountIds : [],
  };
}
