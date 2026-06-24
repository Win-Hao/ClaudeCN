import { QueryClient } from "@tanstack/react-query";

// 本地 IPC 数据：不会自己过期，靠 mutation 后显式 invalidate 刷新。
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: Infinity,
      refetchOnWindowFocus: false,
      retry: false,
    },
  },
});
