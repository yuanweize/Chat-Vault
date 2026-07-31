import { save } from "@tauri-apps/plugin-dialog";
import { writeFile, readFile, readTextFile } from "@tauri-apps/plugin-fs";
import { appDataDir, join } from "@tauri-apps/api/path";
import { openPath } from "@tauri-apps/plugin-opener";
import { Conversation } from "../data/types";
import i18n from "../i18n";

function safeFileName(value: string): string {
  const normalized = value
    .replace(/[\\/:*?"<>|\u0000-\u001f]/g, "-")
    .replace(/\.{2,}/g, ".")
    .trim()
    .replace(/[. ]+$/g, "");
  return normalized.slice(0, 120) || "conversation";
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let val = bytes;
  let idx = 0;
  while (val >= 1024 && idx < units.length - 1) {
    val /= 1024;
    idx += 1;
  }
  const fixed = idx === 0 ? 0 : (val >= 100 ? 0 : 1);
  return `${val.toFixed(fixed)} ${units[idx]}`;
}

export async function exportConversationToZip(conversation: Conversation, accountId: string) {
  try {
    const { default: JSZip } = await import("jszip");
    const baseDir = await appDataDir();
    const bareId = conversation.id.replace(/^c_/, "");
    
    // Read the automatically generated Markdown file
    const mdPath = await join(baseDir, "accounts", accountId, "exports", "markdown", `${bareId}.md`);
    let mdText = "";
    try {
      mdText = await readTextFile(mdPath);
    } catch (e) {
      console.error("Failed to read MD file", e);
      throw new Error(i18n.t("app.markdownMissing"));
    }

    // Rewrite media paths
    mdText = mdText.replace(/\.\.\/\.\.\/media\//g, "assets/");

    const zip = new JSZip();
    const exportName = safeFileName(conversation.title);
    zip.file(`${exportName}.md`, mdText);

    // Collect all media IDs used in this conversation
    const mediaIds = new Set<string>();
    for (const msg of conversation.messages) {
      if (msg.attachments) {
        for (const att of msg.attachments) {
          if (att.mediaId) {
            mediaIds.add(att.mediaId);
          }
          if (att.previewMediaId) mediaIds.add(att.previewMediaId);
        }
      }
      for (const canvas of msg.canvas ?? []) {
        if (canvas.content_media_id) mediaIds.add(canvas.content_media_id);
      }
      if (msg.deepResearch?.report_media_id) mediaIds.add(msg.deepResearch.report_media_id);
      if (msg.deepResearch?.progress_media_id) mediaIds.add(msg.deepResearch.progress_media_id);
    }

    // Read media files and add to zip
    if (mediaIds.size > 0) {
      const assets = zip.folder("assets");
      if (assets) {
        for (const mediaId of mediaIds) {
          try {
            const mediaPath = await join(baseDir, "accounts", accountId, "media", mediaId);
            const bytes = await readFile(mediaPath);
            assets.file(mediaId, bytes);
          } catch (e) {
            console.warn(`Failed to read media ${mediaId}`, e);
          }
        }
      }
    }

    const ext = mediaIds.size > 0 ? "zip" : "md";
    const savePath = await save({
      defaultPath: `${exportName}.${ext}`,
      filters: ext === "zip" ? [{ name: i18n.t("app.zipArchive"), extensions: ["zip"] }] : [{ name: "Markdown", extensions: ["md"] }],
    });

    if (savePath) {
      if (ext === "zip") {
        const zipBytes = await zip.generateAsync({ type: "uint8array" });
        await writeFile(savePath, zipBytes);
      } else {
        // If no media, just save the markdown directly
        const encoder = new TextEncoder();
        await writeFile(savePath, encoder.encode(mdText));
      }
      return true; // Success
    }
    return false; // Canceled
  } catch (error) {
    console.error("Export failed:", error);
    throw error;
  }
}

export async function openExportDirectory(accountId: string) {
  try {
    const baseDir = await appDataDir();

    
    // In Tauri, reading a full directory recursively isn't natively exposed in a single call in v2 without plugins,
    // so the best way is to trigger a backend rust command to zip the folder, OR just tell the user where the folder is.
    // However, since we want to package it, let's open the system file explorer to the exports folder!
    const exportsDir = await join(baseDir, "accounts", accountId, "exports");
    await openPath(exportsDir);
    return true;
  } catch (err) {
    console.error("Failed to open exports directory:", err);
    throw err;
  }
}
