import { useEffect, useRef } from "react";
import { listen, type Event } from "@tauri-apps/api/event";

// 后端 emit 的事件名集中此处，避免拼写漂移。
export type TauriEventName =
  | "localize-progress"
  | "localize-done"
  | "localize-error";

/**
 * 订阅单个 Tauri 事件。handler 用 ref 存最新值，避免因 handler 变化而反复重订阅；
 * 仅在挂载时 listen、卸载时 unlisten。
 */
export function useTauriEvent<T>(
  event: TauriEventName,
  handler: (payload: T, raw: Event<T>) => void,
) {
  const ref = useRef(handler);
  ref.current = handler;

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    listen<T>(event, (e) => ref.current(e.payload, e))
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [event]);
}
