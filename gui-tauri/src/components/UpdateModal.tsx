import { AnimatePresence, motion } from "framer-motion";
import { AlertTriangle, ArrowRight, CheckCircle, Download, RotateCw, Sparkles, X } from "lucide-react";
import type { UpdateState } from "@/lib/useUpdater";

type Props = {
  open: boolean;
  state: UpdateState;
  onUpdate: () => void; // 立即更新（下载+安装+重启）
  onLater: () => void; // 稍后（关闭 + 进入冷却期）
  onSkip: () => void; // 跳过此版本
};

/**
 * 发现新版本弹窗（GUI 自身的应用内自动更新）。
 * 启动静默检查到更新且未被「跳过 / 冷却」则浮现；点「立即更新」走 plugin-updater
 * 下载→安装→重启，全程弹窗内显示进度。
 */
export function UpdateModal({ open, state, onUpdate, onLater, onSkip }: Props) {
  const busy = state.isDownloading || state.isInstalling || state.isRestarting;
  const manualRestart = state.requiresManualRestart;

  const primaryLabel = state.isDownloading
    ? `下载中 ${state.downloadProgress}%`
    : state.isInstalling
      ? "安装中…"
      : state.isRestarting
        ? "重启中…"
        : "立即更新";

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          className="fixed inset-0 z-50 flex items-center justify-center p-6"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.12 }}
        >
          <button
            className="absolute inset-0 bg-black/40 backdrop-blur-[1px]"
            aria-label="稍后"
            onClick={busy ? undefined : onLater}
          />
          <motion.div
            className="elev-pop relative flex w-full max-w-md flex-col overflow-hidden rounded-2xl border border-[color:var(--border)] bg-[var(--surface)] text-[color:var(--fg)]"
            initial={{ opacity: 0, y: 10, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 10, scale: 0.98 }}
            transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
          >
            {/* 头部 */}
            <div className="flex items-start justify-between gap-3 px-5 pt-5 pb-3">
              <div className="flex items-center gap-2.5">
                <span className="grid h-9 w-9 shrink-0 place-items-center rounded-full bg-[var(--accent-soft)] text-[color:var(--accent)]">
                  <Sparkles size={18} />
                </span>
                <div>
                  <div className="text-[15px] font-semibold">发现新版本</div>
                  <div className="text-xs text-[color:var(--fg-3)]">
                    ClaudeCN v{state.newVersion ?? "?"}
                  </div>
                </div>
              </div>
              {!busy && (
                <button className="btn-del" onClick={onLater} aria-label="稍后">
                  <X size={15} />
                </button>
              )}
            </div>

            {/* 版本对比 */}
            <div className="mx-5 mb-3 flex items-center justify-center gap-4 rounded-lg border border-[color:var(--hairline)] bg-[var(--surface-hover)] py-3">
              <div className="text-center">
                <div className="text-[11px] text-[color:var(--fg-3)]">当前</div>
                <div className="text-sm font-medium">{state.currentVersion || "—"}</div>
              </div>
              <ArrowRight size={16} className="text-[color:var(--fg-3)]" />
              <div className="text-center">
                <div className="text-[11px] text-[color:var(--fg-3)]">最新</div>
                <div className="text-sm font-semibold text-[color:var(--accent)]">
                  {state.newVersion ?? "—"}
                </div>
              </div>
            </div>

            {/* 更新说明 */}
            {state.notes?.trim() && (
              <div className="mx-5 mb-4">
                <div className="mb-1.5 text-[11px] font-semibold tracking-wide text-[color:var(--fg-2)]">
                  更新内容
                </div>
                <div className="max-h-32 overflow-y-auto whitespace-pre-wrap rounded-lg border border-[color:var(--hairline)] bg-[var(--surface-hover)] px-3 py-2.5 text-xs leading-relaxed text-[color:var(--fg-2)]">
                  {state.notes.trim()}
                </div>
              </div>
            )}

            {/* 下载进度 */}
            {state.isDownloading && (
              <div className="mx-5 mb-4 space-y-1.5">
                <div className="flex items-center gap-2 text-xs text-[color:var(--fg-2)]">
                  <Download size={13} className="animate-bounce" />
                  下载中 {state.downloadProgress}%
                </div>
                <div className="h-1.5 w-full overflow-hidden rounded-full bg-[var(--surface-hover)]">
                  <div
                    className="h-full rounded-full bg-[var(--accent)] transition-[width] duration-150 ease-out"
                    style={{ width: `${state.downloadProgress}%` }}
                  />
                </div>
              </div>
            )}

            {/* 安装中 / 重启中 */}
            {(state.isInstalling || state.isRestarting) && !manualRestart && (
              <div className="mx-5 mb-4 flex items-center gap-2 text-xs text-[color:var(--fg-2)]">
                <RotateCw size={13} className="animate-spin text-[color:var(--accent)]" />
                {state.isRestarting ? "安装完成，正在重启…" : "安装中…"}
              </div>
            )}

            {/* 已下载但需手动重启 */}
            {manualRestart && (
              <div className="mx-5 mb-4 rounded-lg border border-emerald-500/30 bg-emerald-500/10 px-3 py-2.5">
                <div className="flex items-center gap-2 text-xs font-medium text-emerald-600 dark:text-emerald-400">
                  <CheckCircle size={14} className="shrink-0" />
                  更新已下载完成
                </div>
                <p className="mt-1 pl-6 text-[11px] text-[color:var(--fg-3)]">
                  请手动退出 ClaudeCN（⌘Q）后重新打开即可用上新版本。
                </p>
              </div>
            )}

            {/* 出错 */}
            {state.error && !busy && !manualRestart && (
              <div className="mx-5 mb-4 rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2.5">
                <div className="flex items-center gap-2 text-xs text-red-600 dark:text-red-400">
                  <AlertTriangle size={14} className="shrink-0" />
                  更新出错：{state.error}
                </div>
                <p className="mt-1 text-[11px] text-[color:var(--fg-3)]">
                  可稍后重试，或到关于页前往 GitHub 手动下载。
                </p>
              </div>
            )}

            {/* 操作 */}
            <div className="flex flex-col gap-2 px-5 pb-5">
              {manualRestart ? (
                <button
                  className="inline-flex w-full items-center justify-center gap-1.5 rounded-lg bg-[var(--accent)] py-2.5 text-sm font-medium text-white transition hover:bg-[var(--accent-hover)]"
                  onClick={onLater}
                >
                  知道了
                </button>
              ) : (
                <>
                  <button
                    className="inline-flex w-full items-center justify-center gap-1.5 rounded-lg bg-[var(--accent)] py-2.5 text-sm font-medium text-white transition hover:bg-[var(--accent-hover)] disabled:opacity-60"
                    onClick={onUpdate}
                    disabled={busy}
                  >
                    {busy ? <RotateCw size={15} className="animate-spin" /> : <Download size={15} />}
                    {primaryLabel}
                  </button>
                  {!busy && (
                    <div className="flex items-center justify-center gap-4 text-xs">
                      <button
                        className="text-[color:var(--fg-3)] transition hover:text-[color:var(--fg-2)]"
                        onClick={onLater}
                      >
                        稍后提醒
                      </button>
                      <span className="text-[color:var(--hairline)]">·</span>
                      <button
                        className="text-[color:var(--fg-3)] transition hover:text-[color:var(--fg-2)]"
                        onClick={onSkip}
                      >
                        跳过此版本
                      </button>
                    </div>
                  )}
                </>
              )}
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
