# ClaudeCN — 官网（React + TypeScript + Vite）

「ClaudeCN」Claude Desktop 中文汉化工具的单页官网，沿用 untype 官网的 **Atelier Zero** 编辑拼贴视觉语言，**精简 + 少图版**：只有 4 个版块，配图除一张主界面截图外全部用内联 SVG 矢量插画。

## 快速开始

```bash
cd site
npm install      # 安装 react / react-dom / gsap / vite / typescript + 类型
npm run dev      # 本地开发，默认 http://localhost:5173
npm run build    # tsc 类型检查 + Vite 产出静态站到 dist/
npm run preview  # 本地预览构建产物
npm run typecheck # 只跑 tsc 类型检查
```

## 技术栈

- **Vite 5** + **React 18** + **TypeScript**（strict）
- **GSAP + ScrollTrigger**：滚动揭示动画，尊重 `prefers-reduced-motion`，异常时降级为直接显示。
- 纯 CSS 设计令牌（`src/index.css`，Atelier Zero 画板）。

## 版块（精简）

`Hero（主界面截图）` → `功能（4 卡）` → `使用流程（4 步横排）` → `下载 CTA` → `页脚`，外加导航 / 顶栏 / 侧栏。

## 目录结构

```
site/
├── index.html                # Vite 入口（加载 src/main.tsx）
├── vite.config.ts            # 生产 base = /ClaudeCN/
├── tsconfig.json             # TS 严格模式
├── public/assets/            # 仅 2 个静态资源：logo + 一张主界面截图
│   ├── logo.png
│   └── app-main.png
└── src/
    ├── main.tsx              # 挂载点
    ├── App.tsx               # 组合所有版块
    ├── index.css             # Atelier Zero 样式表 + ClaudeCN 适配
    ├── vite-env.d.ts         # vite/client 类型
    ├── lib/
    │   ├── plates.ts         # 内联 SVG 插画生成器（A→中 母题）
    │   ├── links.ts          # 对外链接唯一来源
    │   ├── asset.ts          # 资源路径（import.meta.env.BASE_URL）
    │   └── scroll.ts         # 回到顶部
    ├── hooks/                # useReveals / useHeadroom / useLatestDmg
    └── components/           # Nav / Topbar / SideRails / Hero / Features / Steps / CTA / Footer / Plate / BrandMark / StarButton
```

## 设计说明

- **强调色**：珊瑚红 `#ed6f5c`，与 App 图标一致，单一强调色。
- **字体**：Playfair Display（衬线斜体强调）+ Inter Tight / Inter + PingFang SC（中文）。
- **图片**：仅一张主界面真机截图（Hero）；其余插画为 `src/lib/plates.ts` 内联 SVG（统一「英文 A → 中文 中」母题），整站体积极小、清晰可缩放。
- **下载按钮**：macOS 经 GitHub API 自动取最新 release 的 `.dmg`（`hooks/useLatestDmg.ts`，失败回退 `links.ts` 直链）；Windows 走 Releases 页。
- **对外链接**统一在 `src/lib/links.ts`；图片路径走 `src/lib/asset.ts`，整站可在任意 `base` 下正确加载。

## 部署

生产构建 `base` 为 `/ClaudeCN/`（见 `vite.config.ts`），由 `.github/workflows/deploy-site.yml` 在 push 到 `site/**` 时自动构建并部署到 GitHub Pages
<https://win-hao.github.io/ClaudeCN/>。迁到根域名 / 自定义域名时把生产 `base` 改回 `/` 即可。
