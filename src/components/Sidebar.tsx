import { useEffect, useRef, useState, useCallback, useMemo } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { GroupedVirtuoso } from "react-virtuoso";
import { ConversationSummary, Account, SearchResult, Folder } from "../data/types";
import { useTheme } from "../theme";
import { DRAG_REGION_HEIGHT } from "../utils/platform";
import { formatDateTime } from "../utils/dateTime";
import { ImportIcon, ExportIcon, TrashIcon, CopyIcon, CheckIcon, SearchIcon, SyncIcon, FolderIcon, FilterIcon } from "./Icons";
import { useTranslation } from "react-i18next";

interface SidebarProps {
  conversations: ConversationSummary[];
  conversationSortMode?: "updated_desc" | "size_desc" | "media_desc" | "created_desc";
  onToggleConversationSort?: () => void;
  selectedId: string | null;
  onSelect: (id: string, messageId?: string) => void;
  collapsed: boolean;
  listSyncing: boolean;
  fullSyncing: boolean;
  onSyncList: () => void;
  onSyncFull: () => void;
  importingAccountData?: boolean;
  onImport?: () => void;
  exportingAccountData?: boolean;
  onOpenExportModal?: () => void;
  clearingAccountData: boolean;
  disableClearAccountData?: boolean;
  onClearAccountData: () => void;
  currentAccount: Account;
  accounts: Account[];
  onSwitchAccount: (account: Account) => void;
  disableAccountSwitch?: boolean;
  disableConversationSync?: boolean;
  onSyncConversation?: (id: string) => Promise<void> | void;
  syncingConversationIds?: string[];
  onDeleteConversation?: (convId: string) => void;
  onMoveToFolder?: (convId: string, folderId: string | null) => void;
  onCancelList?: () => void;
  onCancelFull?: () => void;
  width?: number;
}

export function Sidebar({
  conversations, selectedId, onSelect, collapsed,
  conversationSortMode = "updated_desc", onToggleConversationSort,
  listSyncing, fullSyncing, onSyncList, onSyncFull, clearingAccountData, onClearAccountData,
  importingAccountData = false, onImport,
  exportingAccountData = false, onOpenExportModal,
  disableClearAccountData = false,
  currentAccount, accounts, onSwitchAccount,
  disableAccountSwitch = false, disableConversationSync = false,
  onSyncConversation, syncingConversationIds = [],
  onDeleteConversation, onMoveToFolder,
  onCancelList,
  onCancelFull,
  width = 280,
}: SidebarProps) {
  const tTheme = useTheme();
  const { t } = useTranslation();
  const [showSwitcher, setShowSwitcher] = useState(false);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [viewMode, setViewMode] = useState<"timeline" | "folders">("timeline");
  const [selectedFolderId, setSelectedFolderId] = useState<string | null>(null);
  const [filterMode, setFilterMode] = useState<"all" | "alive" | "deleted">("all");
  const [showFilterMenu, setShowFilterMenu] = useState(false);
  const filterTriggerRef = useRef<HTMLButtonElement | null>(null);
  const switcherTriggerRef = useRef<HTMLDivElement>(null);
  const [switcherRect, setSwitcherRect] = useState<{ left: number; top: number; width: number } | null>(null);
  const updateSwitcherRect = useCallback(() => {
    const el = switcherTriggerRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setSwitcherRect({
      left: r.left + 6,
      top: r.top - 2,
      width: r.width - 12,
    });
  }, []);
  const [filterMenuRect, setFilterMenuRect] = useState<{ left: number; top: number; width: number } | null>(null);
  const updateFilterMenuRect = useCallback(() => {
    const el = filterTriggerRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setFilterMenuRect({
      left: r.left - 100, // Shift left to align
      top: r.bottom + 8,
      width: 140,
    });
  }, []);
  useEffect(() => {
    if (!showFilterMenu) return;
    updateFilterMenuRect();
    const onResize = () => updateFilterMenuRect();
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [showFilterMenu, updateFilterMenuRect]);
  
  useEffect(() => {
    if (!showSwitcher) return;
    updateSwitcherRect();
    const onResize = () => updateSwitcherRect();
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [showSwitcher, updateSwitcherRect]);
  const [cancelConfirm, setCancelConfirm] = useState<"list" | "full" | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; convId: string } | null>(null);
  const [showSearch, setShowSearch] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<SearchResult[]>([]);
  const [searching, setSearching] = useState(false);
  const searchTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const searchPanelRef = useRef<HTMLDivElement | null>(null);

  const syncingSet = new Set(syncingConversationIds);

  useEffect(() => {
    invoke<Folder[]>("get_folders", { accountId: currentAccount.id })
      .then(setFolders)
      .catch(e => console.error("加载文件夹失败:", e));
  }, [currentAccount.id]);

  async function handleCreateFolder() {
    const name = window.prompt(t("sidebar.enterFolderName"));
    if (!name?.trim()) return;
    const newFolder: Folder = { id: Date.now().toString(), name: name.trim() };
    const newFolders = [...folders, newFolder];
    setFolders(newFolders);
    await invoke("save_folders", { accountId: currentAccount.id, folders: newFolders });
  }

  async function handleDeleteFolder(id: string) {
    if (!window.confirm(t("sidebar.confirmDeleteFolder"))) return;
    const newFolders = folders.filter(f => f.id !== id);
    setFolders(newFolders);
    await invoke("save_folders", { accountId: currentAccount.id, folders: newFolders });
    if (selectedFolderId === id) setSelectedFolderId(null);
  }

  const groupedConversations = useMemo(() => {
    let filtered = conversations;
    if (filterMode === "alive") {
      filtered = filtered.filter(c => c.status !== "lost");
    } else if (filterMode === "deleted") {
      filtered = filtered.filter(c => c.status === "lost");
    }
    if (selectedFolderId) {
      filtered = filtered.filter(c => c.folderId === selectedFolderId);
    }

    if (conversationSortMode !== "created_desc" && conversationSortMode !== "updated_desc") {
      return { groupCounts: [filtered.length], groupTitles: [""], items: filtered };
    }
    
    const items: ConversationSummary[] = [];
    const groupTitles: string[] = [];
    const groupCounts: number[] = [];
    
    let currentTitle = "";
    let currentCount = 0;
    
    const now = new Date();
    const currentYear = now.getFullYear();
    const currentMonth = now.getMonth();
    
    filtered.forEach(conv => {
      const dateStr = conversationSortMode === "created_desc" && conv.createdAt ? conv.createdAt : conv.updatedAt;
      let title = t("sidebar.earlier");
      if (dateStr) {
        const d = new Date(dateStr);
        if (!isNaN(d.getTime())) {
          const m = d.getMonth();
          const y = d.getFullYear();
          if (y === currentYear && m === currentMonth) {
            title = t("sidebar.this_month");
          } else if (y === currentYear && m === currentMonth - 1) {
            title = t("sidebar.last_month");
          } else if (y === currentYear) {
            title = t("sidebar.month", { month: m + 1 });
          } else {
            title = t("sidebar.yearMonth", { year: y, month: m + 1 });
          }
        }
      }
      
      if (title !== currentTitle) {
        if (currentCount > 0) {
          groupTitles.push(currentTitle);
          groupCounts.push(currentCount);
        }
        currentTitle = title;
        currentCount = 1;
      } else {
        currentCount++;
      }
      items.push(conv);
    });
    
    if (currentCount > 0) {
      groupTitles.push(currentTitle);
      groupCounts.push(currentCount);
    }
    
    // Fallback if empty
    if (items.length === 0) {
      return { groupCounts: [], groupTitles: [], items: [] };
    }
    
    return { items, groupTitles, groupCounts };
  }, [conversations, conversationSortMode, filterMode, selectedFolderId, t]);

  const doSearch = useCallback(async (q: string) => {
    if (!q.trim()) {
      setSearchResults([]);
      setSearching(false);
      return;
    }
    setSearching(true);
    try {
      const raw = await invoke<string>("search_conversations", {
        accountId: currentAccount.id,
        query: q.trim(),
        limit: 50,
      });
      setSearchResults(JSON.parse(raw) as SearchResult[]);
    } catch (e) {
      console.error("搜索失败:", e);
      setSearchResults([]);
    } finally {
      setSearching(false);
    }
  }, [currentAccount.id]);

  // debounced search
  useEffect(() => {
    if (searchTimerRef.current) clearTimeout(searchTimerRef.current);
    if (!searchQuery.trim()) {
      setSearchResults([]);
      return;
    }
    searchTimerRef.current = setTimeout(() => { void doSearch(searchQuery); }, 300);
    return () => { if (searchTimerRef.current) clearTimeout(searchTimerRef.current); };
  }, [searchQuery, doSearch]);

  // 切换账号时清空搜索
  useEffect(() => {
    setShowSearch(false);
    setSearchQuery("");
    setSearchResults([]);
  }, [currentAccount.id]);

  // 搜索弹窗打开时聚焦输入框
  useEffect(() => {
    if (showSearch) {
      requestAnimationFrame(() => searchInputRef.current?.focus());
    }
  }, [showSearch]);

  // 点击弹窗外部关闭搜索
  useEffect(() => {
    if (!showSearch) return;
    function handleClickOutside(e: MouseEvent) {
      if (searchPanelRef.current && !searchPanelRef.current.contains(e.target as Node)) {
        setShowSearch(false);
        setSearchQuery("");
        setSearchResults([]);
      }
    }
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        setShowSearch(false);
        setSearchQuery("");
        setSearchResults([]);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [showSearch]);
  const otherAccounts = accounts.filter((a) => a.id !== currentAccount.id);
  const conversationSortTitle =
    conversationSortMode === "size_desc"
      ? t("sidebar.sortTipCount")
      : conversationSortMode === "media_desc"
        ? t("sidebar.sortTipMedia")
        : conversationSortMode === "created_desc"
          ? t("sidebar.sortTipCreated")
          : t("sidebar.sortTipUpdated");
  const conversationSortLabel =
    conversationSortMode === "size_desc"
      ? t("sidebar.sortByCount")
      : conversationSortMode === "media_desc"
        ? t("sidebar.sortByMedia")
        : conversationSortMode === "created_desc"
          ? t("sidebar.sortByCreated")
          : t("sidebar.sortByUpdated");

  useEffect(() => {
    if (disableAccountSwitch && showSwitcher) {
      setShowSwitcher(false);
    }
  }, [disableAccountSwitch, showSwitcher]);

  function handleSyncConv(id: string) {
    if (disableConversationSync || syncingSet.has(id)) return;
    void Promise.resolve(onSyncConversation?.(id)).catch((e) => {
      console.error("同步单对话失败:", e);
    });
  }

  return (
    <div
      id="sidebar-root"
      onClick={() => setContextMenu(null)}
      style={{
      width: collapsed ? 0 : width,
      minWidth: collapsed ? 0 : width,
      transition: "width 0.25s cubic-bezier(0.4,0,0.2,1), min-width 0.25s cubic-bezier(0.4,0,0.2,1)",
      overflow: "hidden",
      background: tTheme.sidebarBg,
      borderRight: `1px solid ${tTheme.border}`,
      display: "flex",
      flexDirection: "column",
      flexShrink: 0,
      position: "relative",
    }}>
      <div data-tauri-drag-region style={{ height: DRAG_REGION_HEIGHT, minWidth: width, flexShrink: 0 }} />

      <div style={{ flex: 1, minHeight: 0, padding: "0 0 4px", minWidth: width, display: "flex", flexDirection: "column" }}>
        <div style={{ padding: "2px 12px 6px 14px", display: "flex", alignItems: "center", justifyContent: "space-between", gap: 8 }}>
          <span style={{ fontSize: 11, fontWeight: 600, color: tTheme.textMuted, letterSpacing: 0.5, textTransform: "uppercase" }}>
            {t("sidebar.title")}
          </span>
          <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
            <button
              title={t("sidebar.search")}
              onClick={(e) => {
                e.stopPropagation();
                setShowSearch(true);
              }}
              style={{
                width: 22,
                height: 22,
                borderRadius: 6,
                border: "none",
                background: "transparent",
                cursor: "pointer",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                flexShrink: 0,
                transition: "background 0.12s",
              }}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLElement).style.background = tTheme.btnHoverBg;
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLElement).style.background = "transparent";
              }}
            >
              <SearchIcon color={tTheme.textMuted} />
            </button>
            {/* 导入按钮 */}
            <button
              title={t("sidebar.importZip")}
              onClick={(e) => {
                e.stopPropagation();
                onImport?.();
              }}
              style={{
                width: 22,
                height: 22,
                borderRadius: 6,
                border: "none",
                background: "transparent",
                cursor: importingAccountData ? "default" : "pointer",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                flexShrink: 0,
                opacity: importingAccountData ? 0.62 : 1,
                transition: "background 0.12s",
              }}
              onMouseEnter={(e) => {
                if (!importingAccountData) (e.currentTarget as HTMLElement).style.background = tTheme.btnHoverBg;
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLElement).style.background = "transparent";
              }}
            >
              <ImportIcon spinning={importingAccountData} color={importingAccountData ? "var(--accent)" : tTheme.textMuted} />
            </button>
            {/* 导出按钮 */}
            <button
              title={t("sidebar.exportAccount")}
              onClick={(e) => {
                e.stopPropagation();
                onOpenExportModal?.();
              }}
              style={{
                width: 22,
                height: 22,
                borderRadius: 6,
                border: "none",
                background: "transparent",
                cursor: exportingAccountData ? "default" : "pointer",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                flexShrink: 0,
                opacity: exportingAccountData ? 0.62 : 1,
                transition: "background 0.12s",
              }}
              onMouseEnter={(e) => {
                if (!exportingAccountData) (e.currentTarget as HTMLElement).style.background = tTheme.btnHoverBg;
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLElement).style.background = "transparent";
              }}
            >
              <ExportIcon spinning={exportingAccountData} color={exportingAccountData ? "var(--accent)" : tTheme.textMuted} />
            </button>
            <button
              ref={filterTriggerRef}
              onClick={(e) => {
                e.stopPropagation();
                setShowFilterMenu(true);
              }}
              title={t("sidebar.filterAndStatus")}
              style={{
                width: 22,
                height: 22,
                borderRadius: 6,
                border: "none",
                background: "transparent",
                cursor: "pointer",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                flexShrink: 0,
                color: (filterMode !== "all" || selectedFolderId) ? "var(--accent)" : tTheme.textMuted,
                transition: "background 0.12s",
              }}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLElement).style.background = tTheme.btnHoverBg;
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLElement).style.background = "transparent";
              }}
            >
              <FilterIcon color={(filterMode !== "all" || selectedFolderId) ? "var(--accent)" : tTheme.textMuted} />
            </button>
            <button
              onClick={(e) => {
                e.stopPropagation();
                onToggleConversationSort?.();
              }}
              title={conversationSortTitle}
              style={{
                height: 22,
                borderRadius: 6,
                border: "none",
                background: "transparent",
                cursor: "pointer",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                flexShrink: 0,
                padding: "0 6px",
                color: tTheme.textMuted,
                fontSize: 10.5,
                fontWeight: 700,
                letterSpacing: 0.2,
                transition: "background 0.12s",
              }}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLElement).style.background = tTheme.btnHoverBg;
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLElement).style.background = "transparent";
              }}
            >
              {conversationSortLabel}
            </button>
            <button
              onClick={(e) => {
                e.stopPropagation();
                if (clearingAccountData || disableClearAccountData) return;
                onClearAccountData();
              }}
              title={t("sidebar.clearAccount")}
              style={{
                width: 22,
                height: 22,
                borderRadius: 6,
                border: "none",
                background: "transparent",
                cursor: (clearingAccountData || disableClearAccountData) ? "default" : "pointer",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                flexShrink: 0,
                opacity: (clearingAccountData || disableClearAccountData) ? 0.55 : 1,
                transition: "background 0.12s",
              }}
              onMouseEnter={(e) => {
                if (clearingAccountData || disableClearAccountData) return;
                (e.currentTarget as HTMLElement).style.background = tTheme.btnHoverBg;
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLElement).style.background = "transparent";
              }}
            >
              <TrashIcon color={clearingAccountData ? "#d34b4b" : tTheme.textMuted} />
            </button>
          </div>
        </div>
        {/* 过滤弹窗 */}
        {showFilterMenu && filterMenuRect && createPortal(
          <>
            <div
              style={{ position: "fixed", top: 0, left: 0, right: 0, bottom: 0, zIndex: 9998 }}
              onClick={(e) => { e.stopPropagation(); setShowFilterMenu(false); }}
              onContextMenu={(e) => { e.preventDefault(); setShowFilterMenu(false); }}
            />
            <div style={{
              position: "fixed",
              top: filterMenuRect.top,
              left: filterMenuRect.left,
              width: filterMenuRect.width,
              background: tTheme.cardBg,
              border: `1px solid ${tTheme.border}`,
              borderRadius: 4,
              padding: "4px 0",
              zIndex: 9999,
              fontSize: 12,
              color: tTheme.text,
            }}>
              <div style={{ padding: "4px 12px", color: tTheme.textMuted, fontSize: 10, fontWeight: 600 }}>{t("sidebar.filterStatus")}</div>
              <div
                style={{ padding: "6px 12px", cursor: "pointer", background: filterMode === "all" ? tTheme.hover : "transparent" }}
                onClick={() => { setFilterMode("all"); setShowFilterMenu(false); }}
              >{t("sidebar.filterAll")}</div>
              <div
                style={{ padding: "6px 12px", cursor: "pointer", background: filterMode === "alive" ? tTheme.hover : "transparent" }}
                onClick={() => { setFilterMode("alive"); setShowFilterMenu(false); }}
              >{t("sidebar.filterAlive")}</div>
              <div
                style={{ padding: "6px 12px", cursor: "pointer", background: filterMode === "deleted" ? tTheme.hover : "transparent" }}
                onClick={() => { setFilterMode("deleted"); setShowFilterMenu(false); }}
              >{t("sidebar.filterZombie")}</div>

              <div style={{ margin: "4px 0", borderTop: `1px solid ${tTheme.border}` }} />
              <div style={{ padding: "4px 12px", color: tTheme.textMuted, fontSize: 10, fontWeight: 600 }}>{t("sidebar.folderView")}</div>
              <div
                style={{ padding: "6px 12px", cursor: "pointer", background: viewMode === "timeline" ? tTheme.hover : "transparent" }}
                onClick={() => { setViewMode("timeline"); setShowFilterMenu(false); }}
              >{t("sidebar.timelineView")}</div>
              <div
                style={{ padding: "6px 12px", cursor: "pointer", background: viewMode === "folders" ? tTheme.hover : "transparent" }}
                onClick={() => { setViewMode("folders"); setShowFilterMenu(false); }}
              >{t("sidebar.folderManagement")}</div>
            </div>
          </>,
          document.body
        )}
        {/* 搜索弹窗 — Portal 到 body，全屏居中 */}
        {showSearch && createPortal(
          <div style={{
            position: "fixed",
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            zIndex: 9000,
            background: tTheme.isDark ? "rgba(0,0,0,0.5)" : "rgba(0,0,0,0.22)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}>
            <div
              ref={searchPanelRef}
              style={{
                width: 480,
                maxWidth: "90vw",
                maxHeight: "70vh",
                borderRadius: 8,
                background: tTheme.cardBg,
                border: `1px solid ${tTheme.border}`,
                display: "flex",
                flexDirection: "column",
                overflow: "hidden",
              }}
            >
              <div style={{ padding: "14px 14px 8px", position: "relative" }}>
                <SearchIcon color={tTheme.textMuted} style={{ position: "absolute", left: 24, top: "50%", transform: "translateY(-50%)", pointerEvents: "none" }} />
                <style>{`.search-input::placeholder { color: ${tTheme.textMuted}; opacity: 1; }`}</style>
                <input
                  ref={searchInputRef}
                  className="search-input"
                  type="text"
                  placeholder={t("sidebar.searchPlaceholder")}
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  style={{
                    width: "100%",
                    height: 36,
                    borderRadius: 10,
                    border: `1px solid ${tTheme.divider}`,
                    background: tTheme.isDark ? "rgba(255,255,255,0.12)" : "rgba(0,0,0,0.06)",
                    color: tTheme.text,
                    fontSize: 13,
                    paddingLeft: 32,
                    paddingRight: searchQuery ? 30 : 10,
                    outline: "none",
                    boxSizing: "border-box",
                  }}
                />
                {searchQuery && (
                  <button
                    onClick={() => setSearchQuery("")}
                    style={{ position: "absolute", right: 18, top: "50%", transform: "translateY(-50%)", width: 20, height: 20, borderRadius: 6, border: "none", background: "transparent", cursor: "pointer", display: "flex", alignItems: "center", justifyContent: "center", color: tTheme.textMuted, fontSize: 15 }}
                  >
                    ×
                  </button>
                )}
              </div>
              <div style={{ flex: 1, minHeight: 0, overflowY: "auto", padding: "0 8px 8px" }}>
                {searchQuery.trim() ? (
                  searching ? (
                    <div style={{ padding: "12px 8px", fontSize: 13, color: tTheme.textMuted }}>{t("sidebar.searching")}</div>
                  ) : searchResults.length === 0 ? (
                    <div style={{ padding: "12px 8px", fontSize: 13, color: tTheme.textMuted }}>{t("sidebar.noSearchResults")}</div>
                  ) : (
                    searchResults.map((r, i) => (
                      <div
                        key={`${r.conversationId}-${r.messageId}-${i}`}
                        onClick={() => {
                          onSelect(r.conversationId, r.messageId);
                          setShowSearch(false);
                          setSearchQuery("");
                          setSearchResults([]);
                        }}
                        style={{
                          padding: "10px 10px",
                          borderRadius: 8,
                          margin: "1px 0",
                          cursor: "pointer",
                          background: "transparent",
                          transition: "background 0.12s",
                        }}
                        onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = tTheme.hover; }}
                        onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; }}
                      >
                        <div style={{ fontSize: 13, fontWeight: 600, color: tTheme.text, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", marginBottom: 4 }}>
                          {r.title || r.conversationId}
                        </div>
                        <div
                          style={{ fontSize: 12, color: tTheme.textMuted, lineHeight: 1.5, overflow: "hidden", display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical" }}
                          dangerouslySetInnerHTML={{ __html: r.snippet }}
                        />
                      </div>
                    ))
                  )
                ) : (
                  <div style={{ padding: "12px 8px", fontSize: 13, color: tTheme.textMuted }}>{t("sidebar.inputKeyword")}</div>
                )}
              </div>
            </div>
          </div>,
          document.body
        )}
        {viewMode === "folders" ? (
          <div style={{ flex: 1, overflowY: "auto", padding: "10px" }}>
            <button
              onClick={handleCreateFolder}
              style={{ width: "100%", padding: "8px", background: tTheme.btnHoverBg, border: "none", borderRadius: 8, cursor: "pointer", color: tTheme.text, marginBottom: 10, fontSize: 13, fontWeight: 500 }}
            >
              {t("sidebar.newFolder")}
            </button>
            <div
              style={{ display: "flex", alignItems: "center", padding: "10px 12px", background: selectedFolderId === null ? tTheme.selectedBg : "transparent", cursor: "pointer", borderRadius: 8, marginBottom: 4 }}
              onClick={() => { setSelectedFolderId(null); setViewMode("timeline"); }}
            >
              <FolderIcon color={tTheme.textMuted} />
              <span style={{ color: tTheme.text, fontSize: 13, marginLeft: 8 }}>{t("sidebar.allConversations")}</span>
            </div>
            {folders.map(f => (
               <div
                 key={f.id}
                 style={{ display: "flex", justifyContent: "space-between", alignItems: "center", padding: "8px 12px", background: selectedFolderId === f.id ? tTheme.selectedBg : "transparent", cursor: "pointer", borderRadius: 8, transition: "background 0.12s" }}
                 onClick={() => { setSelectedFolderId(f.id); setViewMode("timeline"); }}
                 onMouseEnter={(e) => { if (selectedFolderId !== f.id) (e.currentTarget as HTMLElement).style.background = tTheme.hover; }}
                 onMouseLeave={(e) => { if (selectedFolderId !== f.id) (e.currentTarget as HTMLElement).style.background = "transparent"; }}
               >
                 <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                   <FolderIcon color={f.color || tTheme.textMuted} />
                   <span style={{ color: tTheme.text, fontSize: 13 }}>{f.name}</span>
                 </div>
                 <button
                   onClick={(e) => { e.stopPropagation(); handleDeleteFolder(f.id); }}
                   style={{ background: "transparent", border: "none", color: "#ef4444", cursor: "pointer", fontSize: 12 }}
                   title={t("sidebar.deleteFolder")}
                 >
                   {t("sidebar.delete")}
                 </button>
               </div>
            ))}
            {folders.length === 0 && (
              <div style={{ padding: "20px", textAlign: "center", fontSize: 12, color: tTheme.textMuted }}>
                {t("sidebar.noFolders")}
              </div>
            )}
          </div>
        ) : conversations.length === 0 ? (
          <div style={{ padding: "10px 14px", fontSize: 12, color: tTheme.textMuted }}>
            {t("sidebar.noDataPull")}
          </div>
        ) : (
          <div style={{ flex: 1, minHeight: 0 }}>
            <GroupedVirtuoso
              style={{ height: "100%", scrollbarGutter: "stable" }}
              groupCounts={groupedConversations.groupCounts}
              increaseViewportBy={{ top: 220, bottom: 420 }}
              groupContent={(index) => {
                const title = groupedConversations.groupTitles[index];
                if (!title) return null;
                return (
                  <div style={{ padding: "12px 14px 4px", fontSize: 12, fontWeight: 600, color: tTheme.textMuted, background: tTheme.cardBg, position: "relative", zIndex: 10 }}>
                    {title}
                  </div>
                );
              }}
              itemContent={(index) => {
                const conv = groupedConversations.items[index];
                return (
                  <ConversationItem
                    conversation={conv}
                    selected={conv.id === selectedId}
                    onClick={() => onSelect(conv.id)}
                    syncing={syncingSet.has(conv.id)}
                    onSync={() => handleSyncConv(conv.id)}
                    sortMode={conversationSortMode}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      setContextMenu({ x: e.clientX, y: e.clientY, convId: conv.id });
                    }}
                  />
                );
              }}
            />
          </div>
        )}
      </div>

      <div
        ref={switcherTriggerRef}
        onMouseEnter={() => {
          if (disableAccountSwitch) return;
          updateSwitcherRect();
          setShowSwitcher(true);
        }}
        onMouseLeave={() => setShowSwitcher(false)}
        style={{ padding: "0 6px 6px", minWidth: width, position: "relative" }}
      >
        {showSwitcher && switcherRect && otherAccounts.length > 0 && createPortal(
          <div
            onMouseEnter={() => setShowSwitcher(true)}
            onMouseLeave={() => setShowSwitcher(false)}
            style={{
              position: "fixed",
              left: switcherRect.left,
              top: switcherRect.top,
              width: switcherRect.width,
              transform: "translateY(-100%)",
              borderRadius: 4,
              background: tTheme.cardBg,
              border: `1px solid ${tTheme.border}`,
              overflow: "hidden",
              zIndex: 2000,
            }}
          >
            {otherAccounts.map((account) => (
              <button
                key={account.id}
                onClick={() => {
                  if (disableAccountSwitch) return;
                  onSwitchAccount(account);
                  setShowSwitcher(false);
                }}
                style={{ display: "flex", width: "100%", alignItems: "center", gap: 10, padding: "8px 10px", border: "none", background: "transparent", cursor: disableAccountSwitch ? "default" : "pointer", textAlign: "left", transition: "background 0.1s", opacity: disableAccountSwitch ? 0.6 : 1 }}
                onMouseEnter={(e) => { if (!disableAccountSwitch) (e.currentTarget as HTMLElement).style.background = tTheme.hover; }}
                onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; }}
              >
                <div style={{ width: 28, height: 28, borderRadius: "50%", background: account.avatarColor, display: "flex", alignItems: "center", justifyContent: "center", color: "#fff", fontWeight: 700, fontSize: 12, flexShrink: 0 }}>
                  {account.avatarText}
                </div>
                <div style={{ flex: 1, overflow: "hidden" }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                    <div style={{ fontSize: 13, fontWeight: 500, color: tTheme.text, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                      {account.name}
                    </div>
                    {account.listSyncPending && <PendingDot />}
                  </div>
                  <div style={{ fontSize: 11, color: tTheme.textSub, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{account.email}</div>
                </div>
              </button>
            ))}
          </div>,
          document.body,
        )}

        <div style={{
          position: "relative",
          borderRadius: 10,
          background: showSwitcher ? tTheme.hover : "transparent",
          transition: "background 0.12s",
          display: "flex",
          alignItems: "center",
          gap: 6,
          padding: "10px 10px",
        }}>
          {cancelConfirm && (
            <div
              style={{
                position: "absolute",
                bottom: "100%",
                left: 0,
                right: 0,
                marginBottom: 4,
                borderRadius: 4,
                background: tTheme.cardBg,
                border: `1px solid ${tTheme.border}`,
                padding: "10px 12px",
                zIndex: 200,
              }}
              onClick={(e) => e.stopPropagation()}
            >
              <div style={{ fontSize: 12, color: tTheme.text, marginBottom: 8 }}>
                {t("sidebar.cancelSync")}
              </div>
              <div style={{ display: "flex", gap: 6 }}>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    setCancelConfirm(null);
                    cancelConfirm === "list" ? onCancelList?.() : onCancelFull?.();
                  }}
                  style={{
                    flex: 1, height: 26, borderRadius: 6, border: "none",
                    background: "#ef4444", color: "#fff",
                    fontSize: 12, fontWeight: 600, cursor: "pointer",
                  }}
                >
                  {t("sidebar.cancel")}
                </button>
                <button
                  onClick={(e) => { e.stopPropagation(); setCancelConfirm(null); }}
                  style={{
                    flex: 1, height: 26, borderRadius: 6,
                    border: `1px solid ${tTheme.divider}`,
                    background: "transparent", color: tTheme.text,
                    fontSize: 12, cursor: "pointer",
                  }}
                >
                  {t("sidebar.continue")}
                </button>
              </div>
            </div>
          )}
          <div style={{ width: 28, height: 28, borderRadius: "50%", background: currentAccount.avatarColor, display: "flex", alignItems: "center", justifyContent: "center", color: "#fff", fontWeight: 700, fontSize: 13, flexShrink: 0 }}>
            {currentAccount.avatarText}
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 6, flex: 1, minWidth: 0 }}>
            <span style={{ fontSize: 13, fontWeight: 500, color: tTheme.text, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {currentAccount.name}
            </span>
            {currentAccount.listSyncPending && <PendingDot />}
          </div>
          <button
            onClick={(e) => {
              e.stopPropagation();
              if (listSyncing) {
                setCancelConfirm(prev => prev === "list" ? null : "list");
                return;
              }
              if (!fullSyncing) onSyncList();
            }}
            title={listSyncing ? t("sidebar.stopListSync") : t("sidebar.syncList")}
            style={{
              height: 22,
              borderRadius: 6,
              border: "none",
              background: "transparent",
              cursor: (listSyncing || !fullSyncing) ? "pointer" : "default",
              display: "flex",
              alignItems: "center",
              gap: 4,
              padding: "0 3px",
              flexShrink: 0,
              color: listSyncing ? "var(--accent)" : tTheme.textSub,
              opacity: fullSyncing && !listSyncing ? 0.65 : 1,
              transition: "background 0.12s",
            }}
            onMouseEnter={(e) => {
              e.stopPropagation();
              if (listSyncing || !fullSyncing) (e.currentTarget as HTMLElement).style.background = tTheme.btnHoverBg;
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLElement).style.background = "transparent";
            }}
          >
            <span style={{ fontSize: 11, fontWeight: 700, lineHeight: 1, letterSpacing: 0.2 }}>{t("sidebar.listShort")}</span>
            <SyncIcon spinning={listSyncing} color={listSyncing ? "var(--accent)" : tTheme.textSub} small />
          </button>
          <button
            onClick={(e) => {
              e.stopPropagation();
              if (fullSyncing) {
                setCancelConfirm(prev => prev === "full" ? null : "full");
                return;
              }
              if (!listSyncing) onSyncFull();
            }}
            title={fullSyncing ? t("sidebar.stopFullSync") : t("sidebar.syncFull")}
            style={{
              height: 22,
              borderRadius: 6,
              border: "none",
              background: "transparent",
              cursor: (fullSyncing || !listSyncing) ? "pointer" : "default",
              display: "flex",
              alignItems: "center",
              gap: 4,
              padding: "0 3px",
              flexShrink: 0,
              color: fullSyncing ? "var(--accent)" : tTheme.textSub,
              opacity: listSyncing && !fullSyncing ? 0.65 : 1,
              transition: "background 0.12s",
            }}
            onMouseEnter={(e) => {
              e.stopPropagation();
              if (fullSyncing || !listSyncing) (e.currentTarget as HTMLElement).style.background = tTheme.btnHoverBg;
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLElement).style.background = "transparent";
            }}
          >
            <span style={{ fontSize: 11, fontWeight: 700, lineHeight: 1, letterSpacing: 0.2 }}>{t("sidebar.fullShort")}</span>
            <SyncIcon spinning={fullSyncing} color={fullSyncing ? "var(--accent)" : tTheme.textSub} small />
          </button>
        </div>
      </div>
      {contextMenu && (
        <div
          style={{
            position: "fixed",
            top: contextMenu.y,
            left: contextMenu.x,
            zIndex: 3000,
            background: tTheme.cardBg,
            borderRadius: 4,
            border: `1px solid ${tTheme.border}`,
            padding: "4px 0",
            minWidth: 140,
          }}
          onClick={(e) => e.stopPropagation()}
        >
          <button
            onClick={() => {
              onDeleteConversation?.(contextMenu.convId);
              setContextMenu(null);
            }}
            style={{
              display: "block",
              width: "100%",
              padding: "8px 14px",
              border: "none",
              background: "transparent",
              color: "#ef4444",
              fontSize: 13,
              textAlign: "left",
              cursor: "pointer",
            }}
          >
            {t("sidebar.deleteConversation")}
          </button>
          {folders.length > 0 && (
            <>
              <div style={{ margin: "4px 0", borderTop: `1px solid ${tTheme.divider}` }} />
              <div style={{ padding: "4px 14px", fontSize: 11, color: tTheme.textMuted, fontWeight: 600 }}>{t("sidebar.moveToFolder")}</div>
              {folders.map(f => (
                <button
                  key={f.id}
                  onClick={() => {
                    onMoveToFolder?.(contextMenu.convId, f.id);
                    setContextMenu(null);
                  }}
                  style={{
                    display: "block",
                    width: "100%",
                    padding: "6px 14px",
                    border: "none",
                    background: "transparent",
                    color: tTheme.text,
                    fontSize: 13,
                    textAlign: "left",
                    cursor: "pointer",
                  }}
                  onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = tTheme.hover; }}
                  onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; }}
                >
                  {f.name}
                </button>
              ))}
              <button
                onClick={() => {
                  onMoveToFolder?.(contextMenu.convId, null);
                  setContextMenu(null);
                }}
                style={{
                  display: "block",
                  width: "100%",
                  padding: "6px 14px",
                  border: "none",
                  background: "transparent",
                  color: tTheme.textMuted,
                  fontSize: 13,
                  textAlign: "left",
                  cursor: "pointer",
                }}
                onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = tTheme.hover; }}
                onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; }}
              >
                {t("sidebar.removeFromFolder")}
              </button>
            </>
          )}
        </div>
      )}
    </div>
  );
}

function ConversationItem({ conversation, selected, onClick, syncing, onSync, sortMode, onContextMenu }: {
  conversation: ConversationSummary;
  selected: boolean;
  onClick: () => void;
  syncing: boolean;
  onSync: () => void;
  sortMode?: string;
  onContextMenu?: (e: React.MouseEvent<HTMLDivElement>) => void;
}) {
  const theme = useTheme();
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const isLost = conversation.status === "lost";
  const lostTitleColor = theme.isDark ? "#f87171" : "#d92d20";
  const lostMetaColor = theme.isDark ? "rgba(248,113,113,0.84)" : "#b42318";

  function handleCopyConversationId() {
    void navigator.clipboard.writeText(conversation.id)
      .then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 850);
      })
      .catch((e) => {
        console.error("复制对话 ID 失败:", e);
      });
  }

  return (
    <div
      onClick={onClick}
      onContextMenu={onContextMenu}
      style={{ display: "flex", alignItems: "center", width: "calc(100% - 12px)", padding: "9px 11px", borderRadius: 10, margin: "1px 6px", background: selected ? theme.selectedBg : "transparent", transition: "background 0.12s", cursor: "pointer", gap: 6 }}
      onMouseEnter={(e) => { if (!selected) (e.currentTarget as HTMLElement).style.background = theme.hover; }}
      onMouseLeave={(e) => { if (!selected) (e.currentTarget as HTMLElement).style.background = "transparent"; }}
    >
      {isLost && (
        <span
          title={t("sidebar.statusLost")}
          style={{ width: 7, height: 7, borderRadius: "50%", background: "var(--danger)", flexShrink: 0 }}
        >
        </span>
      )}
      {conversation.hasFailedData && (
        <span
          title={t("sidebar.failedData")}
          style={{ width: 7, height: 7, borderRadius: "50%", background: "#d58b17", flexShrink: 0 }}
        >
        </span>
      )}
      <div style={{ flex: 1, overflow: "hidden", minWidth: 0 }}>
        <div style={{ fontSize: 13, fontWeight: selected ? 650 : 450, color: isLost ? lostTitleColor : (selected ? theme.selectedText : theme.text), overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", marginBottom: 3 }}>
          {conversation.title}
        </div>
        <div style={{ fontSize: 11, color: isLost ? lostMetaColor : theme.textMuted, display: "flex", alignItems: "center", gap: 4 }}>
          <span>{formatDateTime(sortMode === "created_desc" && conversation.createdAt ? conversation.createdAt : conversation.updatedAt)}</span>
          <span style={{ color: isLost ? lostMetaColor : theme.textMuted, opacity: 0.6 }}>·</span>
          <span>{t("sidebar.messages", { count: conversation.messageCount })}</span>
        </div>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 1, marginLeft: 3, marginRight: -2 }}>
        <button
          onClick={(e) => { e.stopPropagation(); handleCopyConversationId(); }}
          title={copied ? t("sidebar.copied") : t("sidebar.copyId")}
          style={{ width: 24, height: 24, borderRadius: 7, border: "none", background: "transparent", cursor: "pointer", display: "flex", alignItems: "center", justifyContent: "center", flexShrink: 0, transition: "background 0.15s" }}
          onMouseEnter={(e) => { e.stopPropagation(); (e.currentTarget as HTMLElement).style.background = theme.btnHoverBg; }}
          onMouseLeave={(e) => { e.stopPropagation(); (e.currentTarget as HTMLElement).style.background = "transparent"; }}
        >
          {copied ? <CheckIcon color="var(--success)" /> : <CopyIcon color={theme.textMuted} />}
        </button>
        <button
          onClick={(e) => { e.stopPropagation(); onSync(); }}
          title={t("sidebar.syncConversation")}
          style={{ width: 24, height: 24, borderRadius: 7, border: "none", background: "transparent", cursor: syncing ? "default" : "pointer", display: "flex", alignItems: "center", justifyContent: "center", flexShrink: 0, transition: "background 0.15s" }}
          onMouseEnter={(e) => { e.stopPropagation(); if (!syncing) (e.currentTarget as HTMLElement).style.background = theme.btnHoverBg; }}
          onMouseLeave={(e) => { e.stopPropagation(); (e.currentTarget as HTMLElement).style.background = "transparent"; }}
        >
          <SyncIcon spinning={syncing} color={syncing ? "var(--accent)" : theme.textMuted} />
        </button>
      </div>
    </div>
  );
}

function PendingDot() {
  const { t } = useTranslation();
  return (
    <span
      title={t("sidebar.syncIncomplete")}
      style={{
        width: 7,
        height: 7,
        borderRadius: "50%",
        background: "#ef4444",
        boxShadow: "0 0 0 2px rgba(239,68,68,0.16)",
        flexShrink: 0,
      }}
    />
  );
}
