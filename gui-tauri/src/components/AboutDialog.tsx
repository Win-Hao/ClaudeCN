import { useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { X, ExternalLink, Heart, RefreshCw, Check } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";

const REPO = "https://github.com/Win-Hao/ClaudeCN";

export function AboutDialog({
  open,
  onOpenChange,
  version,
  checkForUpdates,
  onUpdateAvailable,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  version: string;
  checkForUpdates: () => Promise<{ version: string } | null>;
  onUpdateAvailable: () => void;
}) {
  const [checking, setChecking] = useState(false);
  const [latest, setLatest] = useState(false);

  async function check() {
    setChecking(true);
    setLatest(false);
    try {
      const u = await checkForUpdates();
      if (u) onUpdateAvailable();
      else setLatest(true);
    } catch {
      setLatest(false);
    } finally {
      setChecking(false);
    }
  }

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 bg-black/40" />
        <Dialog.Content
          className="card elev-pop fixed left-1/2 top-1/2 w-[420px] max-w-[90vw] -translate-x-1/2 -translate-y-1/2 p-0"
          aria-describedby={undefined}
        >
          <div className="flex items-center justify-between px-5 pt-4">
            <Dialog.Title className="text-base font-semibold" style={{ color: "var(--fg)" }}>
              关于 ClaudeCN
            </Dialog.Title>
            <Dialog.Close className="btn-del" aria-label="关闭">
              <X size={16} />
            </Dialog.Close>
          </div>

          <div className="px-5 pb-5 pt-2">
            <p className="desc" style={{ marginTop: 0 }}>
              一键将 Claude Desktop 切换为中文界面。前端 UI、桌面菜单、原生弹窗全覆盖，
              一键可恢复英文原版，完整备份、安全无损。
            </p>

            <div className="card" style={{ marginTop: 14, padding: "12px 14px" }}>
              <div className="flex items-center gap-2 text-[13px]" style={{ color: "var(--fg)" }}>
                <Heart size={14} style={{ color: "var(--red)" }} />
                <span style={{ fontWeight: 600 }}>本软件完全免费</span>
              </div>
              <p className="desc">
                作者：Winhao学AI（抖音号 54927876676）。严禁任何形式的商业使用、倒卖或收费分发。
                若你是付费获得本软件的，说明被骗了，请举报卖家。
              </p>
            </div>

            <div className="card-head" style={{ paddingLeft: 0, paddingTop: 16 }}>
              使用须知
            </div>
            <ul className="space-y-1.5 text-[12px]" style={{ color: "var(--fg-2)", lineHeight: 1.55 }}>
              <li>· 汉化前请在 Claude 内开启<b>开发者模式</b>（Settings → Developer），否则登录账号后服务端语言可能覆盖本地汉化。</li>
              <li>· 汉化期间 Claude 的自动更新会被跳过。想升级：先「恢复原版」→ 让 Claude 更新 → 再汉化。</li>
              <li>· 首次汉化后系统可能要你重新授权一次屏幕录制/麦克风等权限（重签名改了签名身份）。</li>
            </ul>

            <button
              className="btn-text"
              style={{ marginTop: 16 }}
              onClick={() => openUrl(REPO)}
            >
              <ExternalLink size={13} />
              在 GitHub 上查看（{REPO.replace("https://", "")}）
            </button>

            <div className="mt-3 flex items-center justify-between">
              <span className="text-[11px]" style={{ color: "var(--fg-3)" }}>
                ClaudeCN v{version}
              </span>
              <button className="btn-text" onClick={check} disabled={checking}>
                {checking ? (
                  <RefreshCw size={12} className="animate-spin" />
                ) : latest ? (
                  <Check size={12} style={{ color: "#34c759" }} />
                ) : (
                  <RefreshCw size={12} />
                )}
                {checking ? "检查中…" : latest ? "已是最新" : "检查更新"}
              </button>
            </div>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
