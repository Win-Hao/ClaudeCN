import { useCallback, useEffect, useRef, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";

// GUI 自身的应用内自动更新状态机（移植 untype useUpdater）。
export interface UpdateState {
  isChecking: boolean;
  hasUpdate: boolean;
  isDownloading: boolean;
  isInstalling: boolean;
  isRestarting: boolean;
  requiresManualRestart: boolean; // 装好了但自动重启失败，提示用户手动重开
  downloadProgress: number; // 0..100
  error: string | null;
  currentVersion: string;
  newVersion: string | null;
  notes: string | null;
}

export const INITIAL_UPDATE_STATE: UpdateState = {
  isChecking: false,
  hasUpdate: false,
  isDownloading: false,
  isInstalling: false,
  isRestarting: false,
  requiresManualRestart: false,
  downloadProgress: 0,
  error: null,
  currentVersion: "",
  newVersion: null,
  notes: null,
};

function errMessage(e: unknown, fallback: string): string {
  if (e instanceof Error && e.message) return e.message;
  if (typeof e === "string" && e.trim()) return e;
  return fallback;
}

export interface UseUpdaterReturn {
  state: UpdateState;
  checkForUpdates: () => Promise<Update | null>;
  downloadAndInstall: () => Promise<void>;
  dismiss: () => void;
}

export function useUpdater(): UseUpdaterReturn {
  const [state, setState] = useState<UpdateState>({ ...INITIAL_UPDATE_STATE });
  const updateRef = useRef<Update | null>(null);

  useEffect(() => {
    getVersion()
      .then((v) => setState((p) => ({ ...p, currentVersion: v })))
      .catch(() => {
        /* 非关键 */
      });
  }, []);

  const checkForUpdates = useCallback(async (): Promise<Update | null> => {
    setState((p) => ({ ...p, isChecking: true, error: null }));
    try {
      const update = await check();
      updateRef.current = update;
      setState((p) => ({
        ...p,
        isChecking: false,
        hasUpdate: !!update,
        newVersion: update?.version ?? null,
        notes: update?.body ?? null,
        requiresManualRestart: false,
      }));
      return update ?? null;
    } catch (e) {
      setState((p) => ({
        ...p,
        isChecking: false,
        hasUpdate: false,
        error: errMessage(e, "检查更新失败"),
      }));
      return null;
    }
  }, []);

  const downloadAndInstall = useCallback(async () => {
    const update = updateRef.current;
    if (!update) return;

    setState((p) => ({
      ...p,
      isDownloading: true,
      isInstalling: false,
      isRestarting: false,
      requiresManualRestart: false,
      error: null,
      downloadProgress: 0,
    }));

    let total = 0;
    let got = 0;
    let finished = false;

    try {
      await update.downloadAndInstall((ev) => {
        switch (ev.event) {
          case "Started":
            total = ev.data.contentLength ?? 0;
            got = 0;
            setState((p) => ({ ...p, downloadProgress: 0 }));
            break;
          case "Progress":
            got += ev.data.chunkLength ?? 0;
            setState((p) => ({
              ...p,
              downloadProgress: total > 0 ? Math.round((got / total) * 100) : 0,
            }));
            break;
          case "Finished":
            finished = true;
            setState((p) => ({ ...p, isDownloading: false, isInstalling: true, downloadProgress: 100 }));
            break;
        }
      });

      setState((p) => ({ ...p, isInstalling: false, isRestarting: true }));
      await new Promise((r) => setTimeout(r, 500));
      await relaunch();
    } catch (e) {
      // Tauri v2 在 macOS 上 relaunch() 有已知 bug：下载已成功、新包已落盘但重启失败。
      // 下载完成 → 走 Rust force_quit_and_relaunch 兜底重开。
      const downloaded = finished || (total > 0 && got >= total);
      if (downloaded) {
        try {
          await invoke("force_quit_and_relaunch");
          setState((p) => ({
            ...p,
            isDownloading: false,
            isInstalling: false,
            isRestarting: true,
            requiresManualRestart: false,
            error: null,
          }));
          return;
        } catch {
          setState((p) => ({
            ...p,
            isDownloading: false,
            isInstalling: false,
            isRestarting: false,
            requiresManualRestart: true,
          }));
          return;
        }
      }
      setState((p) => ({
        ...p,
        isDownloading: false,
        isInstalling: false,
        error: errMessage(e, "下载失败"),
      }));
    }
  }, []);

  const dismiss = useCallback(() => {
    setState((p) => ({ ...p, hasUpdate: false }));
  }, []);

  return { state, checkForUpdates, downloadAndInstall, dismiss };
}
