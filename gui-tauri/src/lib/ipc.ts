import { invoke } from "@tauri-apps/api/core";

// ============================================================
// 后端 IPC 类型化封装 —— 对应 src-tauri 的 #[tauri::command]。
// 契约见 docs/gui-rebuild-tauri.md §6。
// ============================================================

/** detect_claude 一次性返回，驱动前端状态机。 */
export type ClaudeStatus = {
  /** 是否检测到 Claude 安装。 */
  installed: boolean;
  /** Claude 版本号（CFBundleShortVersionString）；未安装为 null。 */
  version: string | null;
  /** 是否已被本工具汉化（以 en-US.original.json 存在为准）。 */
  patched: boolean;
  /** 是否存在原版备份（新位置或旧 GUI 位置任一）。 */
  has_backup: boolean;
  /** 内置 base 译文对当前版本的覆盖率（百分比）；无法计算为 null。 */
  coverage_pct: number | null;
  /** 是否需要提示用户开启开发者模式。 */
  dev_mode_hint: boolean;
  /** 检测到的 Claude.app 绝对路径；未安装为 null。 */
  app_path: string | null;
};

export type LocalizeResult = {
  ok: boolean;
  coverage_pct: number;
  version: string;
};

// ---- GUI 自身 ----
export const guiVersion = () => invoke<string>("gui_version");

// ---- 汉化流程 ----
export const detectClaude = () => invoke<ClaudeStatus>("detect_claude");
export const applyLocalization = () => invoke<LocalizeResult>("apply_localization");
export const restoreOriginal = (force = false) =>
  invoke<void>("restore_original", { force });
export const openDevModeHelp = () => invoke<void>("open_dev_mode_help");
export const collectLogs = () => invoke<string>("collect_logs");
