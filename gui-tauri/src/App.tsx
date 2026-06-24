import { useEffect, useRef, useState, type ReactNode } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { AnimatePresence, motion } from "framer-motion";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  AlertTriangle,
  Check,
  Copy,
  Download,
  ExternalLink,
  Info,
  Languages,
  Loader2,
  RotateCcw,
} from "lucide-react";

import {
  applyLocalization,
  collectLogs,
  detectClaude,
  guiVersion,
  openDevModeHelp,
  restoreOriginal,
  type ClaudeStatus,
} from "@/lib/ipc";
import { useTauriEvent } from "@/lib/useTauriEvent";
import { useUpdater } from "@/lib/useUpdater";
import { AboutDialog } from "@/components/AboutDialog";
import { UpdateModal } from "@/components/UpdateModal";
import logo from "@/assets/logo.png";

// 更新提醒克制策略（localStorage）：跳过此版本不再自动弹；稍后进入 1 天冷却。
const SKIP_KEY = "claudecn.skipUpdateVersion";
const POSTPONE_KEY = "claudecn.updatePostponedAt";
const POSTPONE_MS = 24 * 60 * 60 * 1000;
function isUpdateSuppressed(version: string): boolean {
  if (localStorage.getItem(SKIP_KEY) === version) return true;
  const at = Number(localStorage.getItem(POSTPONE_KEY) || 0);
  return at > 0 && Date.now() - at < POSTPONE_MS;
}

export default function App() {
  const [version, setVersion] = useState("");
  const [aboutOpen, setAboutOpen] = useState(false);
  const [progress, setProgress] = useState<string[]>([]);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    guiVersion().then(setVersion).catch(() => {});
  }, []);

  // GUI 自更新：启动静默检查，有更新且未被跳过/冷却才自动弹窗。
  const updater = useUpdater();
  const [updateOpen, setUpdateOpen] = useState(false);
  const checkForUpdates = updater.checkForUpdates;
  useEffect(() => {
    checkForUpdates().then((u) => {
      if (u && !isUpdateSuppressed(u.version)) setUpdateOpen(true);
    });
  }, [checkForUpdates]);
  function laterUpdate() {
    localStorage.setItem(POSTPONE_KEY, String(Date.now()));
    setUpdateOpen(false);
  }
  function skipUpdate() {
    const v = updater.state.newVersion;
    if (v) localStorage.setItem(SKIP_KEY, v);
    setUpdateOpen(false);
  }

  const detectQ = useQuery({ queryKey: ["claude"], queryFn: detectClaude });

  useTauriEvent<string>("localize-progress", (msg) => {
    setProgress((p) => [...p, msg]);
  });

  const applyM = useMutation({
    mutationFn: applyLocalization,
    onMutate: () => setProgress([]),
    onSettled: () => detectQ.refetch(),
  });
  const restoreM = useMutation({
    mutationFn: (force: boolean) => restoreOriginal(force),
    onMutate: () => setProgress([]),
    onSettled: () => detectQ.refetch(),
  });

  const working = applyM.isPending || restoreM.isPending;
  const error: string =
    (applyM.isError ? String(applyM.error) : "") ||
    (restoreM.isError ? String(restoreM.error) : "");
  const isDowngradeBlock = error.includes("已中止");

  async function copyLogs() {
    const txt = await collectLogs();
    try {
      await navigator.clipboard.writeText(txt);
    } catch {
      const ta = document.createElement("textarea");
      ta.value = txt;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      ta.remove();
    }
    setCopied(true);
    setTimeout(() => setCopied(false), 1600);
  }

  return (
    <div className="flex h-screen flex-col" style={{ background: "var(--bg)" }}>
      {/* 顶部拖拽条（让 Overlay 标题栏可拖动，红绿灯区留白） */}
      <div data-tauri-drag-region className="h-7 shrink-0" />

      <main className="flex flex-1 flex-col items-center overflow-hidden px-7 pb-5">
        <Header />

        <div className="flex w-full flex-1 flex-col justify-center">
          <AnimatePresence mode="wait">
            {working ? (
              <Fade key="working">
                <WorkingCard progress={progress} />
              </Fade>
            ) : detectQ.isLoading ? (
              <Fade key="loading">
                <Centered>
                  <Loader2 className="animate-spin" size={22} style={{ color: "var(--fg-3)" }} />
                  <p className="desc">正在检测 Claude…</p>
                </Centered>
              </Fade>
            ) : (
              <Fade key="idle">
                <StateView
                  status={detectQ.data}
                  error={error}
                  isDowngradeBlock={isDowngradeBlock}
                  onApply={() => applyM.mutate()}
                  onRestore={(force) => restoreM.mutate(force)}
                  onCopyLogs={copyLogs}
                />
              </Fade>
            )}
          </AnimatePresence>
        </div>

        <Footer
          version={version}
          copied={copied}
          onCopyLogs={copyLogs}
          onAbout={() => setAboutOpen(true)}
        />
      </main>

      <AboutDialog
        open={aboutOpen}
        onOpenChange={setAboutOpen}
        version={version}
        checkForUpdates={updater.checkForUpdates}
        onUpdateAvailable={() => {
          setAboutOpen(false);
          setUpdateOpen(true);
        }}
      />
      <UpdateModal
        open={updateOpen}
        state={updater.state}
        onUpdate={() => updater.downloadAndInstall()}
        onLater={laterUpdate}
        onSkip={skipUpdate}
      />
    </div>
  );
}

// ------------------------------------------------------------------ 顶部

function Header() {
  return (
    <div className="flex flex-col items-center gap-1.5 pb-4 pt-1">
      <img
        src={logo}
        alt="ClaudeCN"
        draggable={false}
        className="h-12 w-12"
        style={{ filter: "drop-shadow(0 2px 6px rgba(0,0,0,0.2))" }}
      />
      <h1 className="text-[17px] font-semibold" style={{ color: "var(--fg)" }}>
        ClaudeCN
      </h1>
      <p className="text-[12px]" style={{ color: "var(--fg-2)" }}>
        Claude Desktop 中文汉化工具
      </p>
    </div>
  );
}

// ------------------------------------------------------------------ 状态分支

function StateView({
  status,
  error,
  isDowngradeBlock,
  onApply,
  onRestore,
  onCopyLogs,
}: {
  status?: ClaudeStatus;
  error: string;
  isDowngradeBlock: boolean;
  onApply: () => void;
  onRestore: (force: boolean) => void;
  onCopyLogs: () => void;
}) {
  if (!status || !status.installed) {
    return (
      <Centered>
        <div className="status-pill" style={{ color: "var(--fg-2)" }}>
          未检测到 Claude Desktop
        </div>
        <p className="desc text-center" style={{ maxWidth: 320 }}>
          请先安装 Claude 桌面端，安装后回到这里即可一键汉化。
        </p>
        <button className="btn-primary mt-2" onClick={() => openUrl("https://claude.ai/download")}>
          <Download size={17} />
          前往 claude.ai 下载
        </button>
      </Centered>
    );
  }

  return (
    <div className="flex w-full flex-col gap-4">
      <StatusCard status={status} />

      {error && <ErrorCard error={error} onCopyLogs={onCopyLogs} />}

      {status.patched ? (
        <div className="flex flex-col gap-2.5">
          <button className="btn-secondary" onClick={() => onRestore(false)}>
            <RotateCcw size={16} />
            恢复英文原版
          </button>
          {isDowngradeBlock && (
            <button
              className="btn-text justify-center"
              onClick={() => onRestore(true)}
              style={{ color: "var(--red)" }}
            >
              仍要强制恢复（可能降级 Claude 版本）
            </button>
          )}
        </div>
      ) : (
        <>
          {status.dev_mode_hint && <DevModeHint />}
          <button className="btn-primary" onClick={onApply}>
            <Languages size={18} />
            一键汉化
          </button>
        </>
      )}
    </div>
  );
}

function StatusCard({ status }: { status: ClaudeStatus }) {
  const cov = status.coverage_pct;
  return (
    <div className="card">
      <div className="ui-row" style={{ paddingBottom: 12 }}>
        <div
          className="grid h-9 w-9 shrink-0 place-items-center rounded-full"
          style={{
            background: status.patched ? "rgba(52,199,89,0.15)" : "var(--accent-soft)",
          }}
        >
          {status.patched ? (
            <Check size={18} style={{ color: "#34c759" }} />
          ) : (
            <Info size={18} style={{ color: "var(--accent)" }} />
          )}
        </div>
        <div className="min-w-0 flex-1">
          <div className="text-[14px] font-semibold" style={{ color: "var(--fg)" }}>
            {status.patched ? "已汉化为中文" : "已检测到 Claude"}
          </div>
          <div className="desc" style={{ marginTop: 1 }}>
            {status.version ? `版本 ${status.version}` : "版本未知"}
            {cov != null && ` · 内置译文覆盖 ${cov}%`}
            {status.has_backup && " · 已有原版备份"}
          </div>
        </div>
      </div>
    </div>
  );
}

function DevModeHint() {
  return (
    <button
      className="card tap text-left"
      style={{ padding: "11px 14px", display: "flex", gap: 10, alignItems: "flex-start" }}
      onClick={() => openDevModeHelp()}
    >
      <AlertTriangle size={15} style={{ color: "#ff9f0a", marginTop: 1, flex: "none" }} />
      <div className="min-w-0">
        <div className="text-[12.5px] font-semibold" style={{ color: "var(--fg)" }}>
          汉化前请先开启「开发者模式」
        </div>
        <div className="desc" style={{ marginTop: 1 }}>
          在 Claude 里 Settings → Developer 打开开关，否则登录账号后界面可能仍是英文。点此查看说明。
        </div>
      </div>
      <ExternalLink size={13} style={{ color: "var(--fg-3)", marginTop: 2, flex: "none" }} />
    </button>
  );
}

function ErrorCard({ error, onCopyLogs }: { error: string; onCopyLogs: () => void }) {
  return (
    <div
      className="card"
      style={{ padding: "12px 14px", borderColor: "var(--red-soft)" }}
    >
      <div className="flex items-start gap-2.5">
        <AlertTriangle size={15} style={{ color: "var(--red)", marginTop: 2, flex: "none" }} />
        <div className="min-w-0 flex-1">
          <div className="text-[12.5px] font-semibold" style={{ color: "var(--fg)" }}>
            处理未完成
          </div>
          <div
            className="desc"
            style={{ marginTop: 2, whiteSpace: "pre-wrap", maxHeight: 150, overflow: "auto" }}
          >
            {error.replace(/^Error:\s*/, "")}
          </div>
          <button className="btn-text" style={{ marginTop: 8 }} onClick={onCopyLogs}>
            <Copy size={12} />
            复制日志
          </button>
        </div>
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ 处理中

function WorkingCard({ progress }: { progress: string[] }) {
  const current = progress[progress.length - 1] ?? "正在准备…";
  const scrollRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [progress.length]);

  return (
    <div className="flex w-full flex-col gap-4">
      <div className="flex flex-col items-center gap-1.5">
        <Loader2 className="animate-spin" size={20} style={{ color: "var(--accent)" }} />
        <div className="text-[14px] font-medium" style={{ color: "var(--fg)" }}>
          {current}
        </div>
        <p className="text-[11.5px]" style={{ color: "var(--fg-3)" }}>
          换入时会弹出系统密码框，请输入以授权修改 Claude
        </p>
      </div>

      <div className="progress-track">
        <div className="progress-fill" />
      </div>

      {progress.length > 0 && (
        <div
          ref={scrollRef}
          className="card"
          style={{
            padding: "10px 12px",
            maxHeight: 132,
            overflow: "auto",
            fontFamily: "var(--font-mono)",
          }}
        >
          {progress.map((line, i) => (
            <div
              key={i}
              className="text-[11px]"
              style={{ color: i === progress.length - 1 ? "var(--fg)" : "var(--fg-3)", lineHeight: 1.7 }}
            >
              {line}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ------------------------------------------------------------------ 页脚 / 工具

function Footer({
  version,
  copied,
  onCopyLogs,
  onAbout,
}: {
  version: string;
  copied: boolean;
  onCopyLogs: () => void;
  onAbout: () => void;
}) {
  return (
    <div className="flex w-full items-center justify-between pt-2">
      <button className="btn-text" onClick={onCopyLogs}>
        <Copy size={12} />
        {copied ? "已复制" : "复制日志"}
      </button>
      <div className="flex items-center gap-3">
        <button className="btn-text" onClick={onAbout}>
          <Info size={12} />
          关于
        </button>
        {version && (
          <span className="text-[11px]" style={{ color: "var(--fg-3)" }}>
            v{version}
          </span>
        )}
      </div>
    </div>
  );
}

function Centered({ children }: { children: ReactNode }) {
  return <div className="flex flex-col items-center justify-center gap-3 py-4">{children}</div>;
}

function Fade({ children }: { children: ReactNode }) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -6 }}
      transition={{ duration: 0.16 }}
      className="w-full"
    >
      {children}
    </motion.div>
  );
}
