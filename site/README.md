# ClaudeCN — 官网（React + Vite）

「ClaudeCN」Claude Desktop 中文汉化工具的单页官网，沿用 untype 官网的 **Atelier Zero** 编辑拼贴视觉语言，**精简 + 少图版**：只有 4 个版块，配图除一张主界面截图外全部用内联 SVG 矢量插画。

## 快速开始

```bash
cd site
npm install      # 安装 react / react-dom / gsap / vite
npm run dev      # 本地开发，默认 http://localhost:5173
npm run build    # 产出静态站到 dist/，可直接部署到任意静态托管
npm run preview  # 本地预览构建产物
```

## 技术栈

- **Vite 5** + **React 18**（JSX，无 TypeScript）
- **GSAP + ScrollTrigger**：滚动揭示动画，尊重 `prefers-reduced-motion`，异常时降级为直接显示。
- 纯 CSS 设计令牌（`src/index.css`，Atelier Zero 画板）。

## 版块（精简）

`Hero（主界面截图）` → `功能（4 卡）` → `使用流程（4 步横排）` → `下载 CTA` → `页脚`，外加导航 / 顶栏 / 侧栏。

## 目录结构

```
site/
├── index.html                # Vite 入口
├── vite.config.js            # 生产 base = /ClaudeCN/
├── public/assets/            # 仅 2 个静态资源：logo + 一张主界面截图
│   ├── logo.png
│   └── app-main.png
└── src/
    ├── App.jsx               # 组合所有版块
    ├── index.css             # Atelier Zero 样式表 + ClaudeCN 适配
    ├── lib/
    │   ├── plates.js         # 内联 SVG 插画生成器（A→中 母题）
    │   ├── links.js          # 对外链接唯一来源
    │   └── asset.js          # 资源路径（import.meta.env.BASE_URL）
    ├── hooks/                # useReveals / useHeadroom
    └── components/           # Nav / Topbar / SideRails / Hero / Features / Steps / CTA / Footer
```

## 设计说明

- **强调色**：珊瑚红 `#ed6f5c`，与 App 图标一致，单一强调色。
- **字体**：Playfair Display（衬线斜体强调）+ Inter Tight / Inter + PingFang SC（中文）。
- **图片**：仅一张主界面真机截图（Hero）；其余插画为 `src/lib/plates.js` 内联 SVG（统一「英文 A → 中文 中」母题），整站体积极小、清晰可缩放。
- **对外链接**：GitHub / 下载 / 更新日志 / 反馈等统一收敛在 `src/lib/links.js`；图片路径统一走 `src/lib/asset.js`，整站可在任意 `base` 下正确加载。

## 部署

生产构建 `base` 为 `/ClaudeCN/`（见 `vite.config.js`），适配 GitHub Pages 项目站点
<https://win-hao.github.io/ClaudeCN/>。也可 `npm run build` 后把 `dist/` 丢到 Vercel / Netlify /
Cloudflare Pages 等任意静态托管；迁到根域名或自定义域名时把生产 `base` 改回 `/` 即可。
