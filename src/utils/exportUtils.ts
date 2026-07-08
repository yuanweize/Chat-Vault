import { save } from "@tauri-apps/plugin-dialog";
import { writeFile, readFile, readTextFile } from "@tauri-apps/plugin-fs";
import { appDataDir, join } from "@tauri-apps/api/path";
import JSZip from "jszip";
import { Conversation } from "../data/types";

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
    const baseDir = await appDataDir();
    const bareId = conversation.id.replace(/^c_/, "");
    
    // Read the automatically generated Markdown file
    const mdPath = await join(baseDir, "accounts", accountId, "exports", "markdown", `${bareId}.md`);
    let mdText = "";
    try {
      mdText = await readTextFile(mdPath);
    } catch (e) {
      console.error("Failed to read MD file", e);
      throw new Error("Markdown export is missing. Please sync the conversation again.");
    }

    // Rewrite media paths
    mdText = mdText.replace(/\.\.\/\.\.\/media\//g, "assets/");

    const zip = new JSZip();
    zip.file(`${conversation.title}.md`, mdText);

    // Collect all media IDs used in this conversation
    const mediaIds = new Set<string>();
    for (const msg of conversation.messages) {
      if (msg.attachments) {
        for (const att of msg.attachments) {
          if (att.mediaId) {
            mediaIds.add(att.mediaId);
          }
        }
      }
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

    const zipBytes = await zip.generateAsync({ type: "uint8array" });
    
    const ext = mediaIds.size > 0 ? "zip" : "md";
    const savePath = await save({
      defaultPath: `${conversation.title}.${ext}`,
      filters: ext === "zip" ? [{ name: "ZIP Archive", extensions: ["zip"] }] : [{ name: "Markdown", extensions: ["md"] }],
    });

    if (savePath) {
      if (ext === "zip") {
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

export async function exportAllToZip(accountId: string) {
  try {
    const baseDir = await appDataDir();

    
    // In Tauri, reading a full directory recursively isn't natively exposed in a single call in v2 without plugins,
    // so the best way is to trigger a backend rust command to zip the folder, OR just tell the user where the folder is.
    // However, since we want to package it, let's open the system file explorer to the exports folder!
    const exportsDir = await join(baseDir, "accounts", accountId, "exports");
    const { openPath } = await import("@tauri-apps/plugin-opener");
    await openPath(exportsDir);
    return true;
  } catch (err) {
    console.error("Failed to open exports directory:", err);
    throw err;
  }
}
