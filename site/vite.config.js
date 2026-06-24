import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vitejs.dev/config/
// 开发态用根路径 '/'；生产构建用 '/ClaudeCN/'，适配 GitHub Pages 项目站点
// （https://win-hao.github.io/ClaudeCN/）。所有图片路径已统一走
// import.meta.env.BASE_URL（见 src/lib/asset.js），改这里即可整体迁移到
// 别的子路径；迁到根域名 / 自定义域名时把生产 base 改回 '/' 即可。
export default defineConfig(({ command }) => ({
  plugins: [react()],
  base: command === 'build' ? '/ClaudeCN/' : '/',
  // 显式给空的内联 postcss 配置，避免 Vite 向上目录搜索误用到别处的 postcss/tailwind 配置。
  css: { postcss: {} },
}))
