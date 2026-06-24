/* ClaudeCN 内联 SVG 插画生成器（纯函数，确定性输出）。
 * Atelier Zero 视觉语言；以矢量插画 + 少量真机截图替代大量配图。
 * 主题统一围绕「英文 A → 中文 中」的转换。
 * 返回 SVG 字符串，组件里用 dangerouslySetInnerHTML 注入。 */

const PAL = {
  paper: '#efe7d2', bone: '#f7f1de', paperDark: '#ddd2b6', stone: '#cdbf9f',
  ink: '#15140f', inkSoft: '#2a2620', inkMute: '#5a5448', inkFaint: '#8b8676',
  coral: '#ed6f5c', mustard: '#e9b94a', olive: '#6e7448',
}
const SANS = "'Inter Tight','Inter',system-ui,sans-serif"
const CJK = "'PingFang SC','Hiragino Sans GB','Microsoft YaHei',sans-serif"
const MONO = "'JetBrains Mono','SF Mono',monospace"

const W = 760, H = 760

function folds(w, h) {
  const x1 = Math.round(w * 0.34), x2 = Math.round(w * 0.68)
  return `<line x1="${x1}" y1="0" x2="${x1}" y2="${h}" stroke="${PAL.inkFaint}" stroke-opacity=".12"/>` +
    `<line x1="${x2}" y1="0" x2="${x2}" y2="${h}" stroke="${PAL.inkFaint}" stroke-opacity=".10"/>`
}
function crosshair(cx, cy, r = 9) {
  return `<g stroke="${PAL.inkFaint}" stroke-width="1" stroke-opacity=".55">` +
    `<line x1="${cx - r}" y1="${cy}" x2="${cx + r}" y2="${cy}"/>` +
    `<line x1="${cx}" y1="${cy - r}" x2="${cx}" y2="${cy + r}"/></g>`
}
function ring(cx, cy, r, col = PAL.inkFaint, op = '.4', sw = 1) {
  return `<circle cx="${cx}" cy="${cy}" r="${r}" fill="none" stroke="${col}" stroke-width="${sw}" stroke-opacity="${op}"/>`
}
function rrect(x, y, w, h, r, opt = {}) {
  const fill = opt.fill || 'none', stroke = opt.stroke || 'none', sw = opt.sw || 1
  const op = opt.op != null ? opt.op : 1
  return `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${r}" fill="${fill}" stroke="${stroke}" stroke-width="${sw}" opacity="${op}"/>`
}
function T(cx, cy, str, size, col, opt = {}) {
  const fam = opt.family || SANS, w = opt.weight || 700, anchor = opt.anchor || 'middle'
  return `<text x="${cx}" y="${cy}" text-anchor="${anchor}" dominant-baseline="central" font-family="${fam}" font-size="${size}" font-weight="${w}" fill="${col}" letter-spacing="${opt.ls || 0}">${str}</text>`
}
function arrowR(x1, y, x2, col, w = 10) {
  const s = 12 + w
  return `<line x1="${x1}" y1="${y}" x2="${x2 - 2}" y2="${y}" stroke="${col}" stroke-width="${w}" stroke-linecap="round"/>` +
    `<path d="M${x2 - s} ${y - s} L${x2} ${y} L${x2 - s} ${y + s}" fill="none" stroke="${col}" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round"/>`
}
function arrowD(cx, y1, y2, col, w = 10) {
  const s = 12 + w
  return `<line x1="${cx}" y1="${y1}" x2="${cx}" y2="${y2 - 2}" stroke="${col}" stroke-width="${w}" stroke-linecap="round"/>` +
    `<path d="M${cx - s} ${y2 - s} L${cx} ${y2} L${cx + s} ${y2 - s}" fill="none" stroke="${col}" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round"/>`
}
function check(cx, cy, r, col, filled) {
  const bg = filled ? `<circle cx="${cx}" cy="${cy}" r="${r}" fill="${col}"/>` : ring(cx, cy, r, col, '1', 2)
  const c = filled ? '#fff' : col, a = r * 0.5
  return bg + `<path d="M${cx - a} ${cy} L${cx - a * 0.25} ${cy + a * 0.7} L${cx + a} ${cy - a * 0.6}" fill="none" stroke="${c}" stroke-width="${Math.max(2, r * 0.18)}" stroke-linecap="round" stroke-linejoin="round"/>`
}
function dots(x, y, r = 6, gap = 22) {
  return [PAL.coral, PAL.mustard, PAL.olive].map((c, i) => `<circle cx="${x + i * gap}" cy="${y}" r="${r}" fill="${c}" opacity=".9"/>`).join('')
}
function refreshArc(cx, cy, r, col, sw = 6) {
  // ~280° 弧 + 端点箭头，表示「重启 / 循环」
  const d = `M ${cx} ${cy - r} A ${r} ${r} 0 1 1 ${cx - r * 0.92} ${cy + r * 0.38}`
  const hx = cx - r * 0.92, hy = cy + r * 0.38
  const head = `M ${hx - 13} ${hy - 12} L ${hx} ${hy + 4} L ${hx + 15} ${hy - 6}`
  return `<path d="${d}" fill="none" stroke="${col}" stroke-width="${sw}" stroke-linecap="round"/>` +
    `<path d="${head}" fill="none" stroke="${col}" stroke-width="${sw}" stroke-linecap="round" stroke-linejoin="round"/>`
}

/* 当前页面用到：p-translate / p-detect / p-coverage / p-done
   + 流程四步 s-install / s-dev / s-localize / s-restart */
export function buildPlate(kind, ds = {}) {
  const fit = ds.fit === 'slice' ? 'xMidYMid slice' : 'xMidYMid meet'
  const bg = `<rect width="${W}" height="${H}" fill="${PAL.bone}"/>` + folds(W, H)
  let scene = ''

  switch (kind) {
    /* 英文 A → 中文 中（核心母题，About 用） */
    case 'p-translate': {
      const cy = H * 0.46
      scene = ring(W * 0.5, H * 0.5, 226, PAL.inkFaint, '.18') +
        ring(W * 0.5, H * 0.5, 150, PAL.coral, '.16') +
        T(W * 0.255, cy, 'A', 250, PAL.ink, { weight: 800 }) +
        arrowR(W * 0.405, cy, W * 0.585, PAL.coral, 12) +
        T(W * 0.74, cy, '中', 240, PAL.ink, { family: CJK, weight: 700 }) +
        T(W * 0.5, H * 0.79, '英文  →  简体中文', 30, PAL.inkMute, { family: MONO, weight: 400, ls: 1 })
      break
    }
    /* 自动检测：雷达环 + 三条状态（Capabilities 用） */
    case 'p-detect': {
      const cx = W * 0.5, cy = H * 0.38
      scene = ring(cx, cy, 172, PAL.inkFaint, '.22') + ring(cx, cy, 118, PAL.ink, '.32') + ring(cx, cy, 64, PAL.coral, '.7', 1.4) +
        `<circle cx="${cx}" cy="${cy}" r="9" fill="${PAL.coral}"/>` +
        `<line x1="${cx}" y1="${cy}" x2="${cx + 150}" y2="${cy - 58}" stroke="${PAL.coral}" stroke-width="3" stroke-linecap="round" stroke-opacity=".7"/>`
      const labels = ['版本', '覆盖', '备份']
      labels.forEach((lb, i) => {
        const y = H * 0.66 + i * 48
        scene += check(W * 0.295, y, 13, PAL.coral, true) +
          T(W * 0.345, y, lb, 24, PAL.inkSoft, { family: CJK, weight: 600, anchor: 'start' }) +
          rrect(W * 0.49, y - 5, W * 0.21, 10, 5, { fill: PAL.inkMute, op: i === 2 ? 0.5 : 0.7 })
      })
      break
    }
    /* 覆盖率矩阵：几乎全填，少数回退英文（Coverage 用） */
    case 'p-coverage': {
      const cols = 9, rows = 9, cell = 46, gap = 12
      const gw = cols * cell + (cols - 1) * gap, gh = rows * cell + (rows - 1) * gap
      const ox = (W - gw) / 2, oy = (H - gh) / 2 - 16
      let g = ''
      for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) {
        const x = ox + c * (cell + gap), y = oy + r * (cell + gap)
        const last = (r * cols + c) >= (rows * cols - 5)   // 末尾几格 = 未覆盖（描边）
        const accent = r === c                              // 对角线 = 珊瑚红强调
        if (last) g += rrect(x, y, cell, cell, 8, { stroke: PAL.inkFaint, sw: 1.4, op: 0.5 })
        else if (accent) g += rrect(x, y, cell, cell, 8, { fill: PAL.coral, op: 0.92 })
        else g += rrect(x, y, cell, cell, 8, { fill: PAL.ink, op: 0.9 })
      }
      scene = g + T(W * 0.5, oy + gh + 46, '已译 ≈ 100% · 未覆盖自动回退英文', 25, PAL.inkMute, { family: MONO, weight: 400 })
      break
    }
    /* 完成：中文就绪 + 对勾徽标（CTA 用） */
    case 'p-done': {
      const cx = W * 0.5, cy = H * 0.45
      scene = ring(cx, cy, 200, PAL.inkFaint, '.2') + ring(cx, cy, 140, PAL.coral, '.18') +
        T(cx, cy, '中', 250, PAL.ink, { family: CJK, weight: 700 }) +
        check(W * 0.685, H * 0.30, 40, PAL.coral, true) +
        T(W * 0.5, H * 0.80, '已就绪 · 中文界面', 28, PAL.inkMute, { family: MONO, weight: 400 })
      break
    }
    /* 步骤 1 · 下载安装 */
    case 's-install': {
      const cx = W * 0.5
      scene = ring(cx, H * 0.42, 168, PAL.inkFaint, '.16') +
        arrowD(cx, H * 0.22, H * 0.54, PAL.coral, 14) +
        `<path d="M${W * 0.30} ${H * 0.56} L${W * 0.30} ${H * 0.70} L${W * 0.70} ${H * 0.70} L${W * 0.70} ${H * 0.56}" fill="none" stroke="${PAL.ink}" stroke-width="6" stroke-linecap="round" stroke-linejoin="round"/>` +
        T(W * 0.385, H * 0.80, 'dmg', 24, PAL.inkMute, { family: MONO, weight: 400 }) +
        T(W * 0.615, H * 0.80, 'exe', 24, PAL.inkMute, { family: MONO, weight: 400 })
      break
    }
    /* 步骤 2 · 开发者模式（开关 ON） */
    case 's-dev': {
      const tw = W * 0.42, th = H * 0.17, tx = (W - tw) / 2, ty = H * 0.43, r = th / 2
      scene = ring(W * 0.5, H * 0.5, 182, PAL.inkFaint, '.16') +
        T(W * 0.5, H * 0.30, '开发者模式', 32, PAL.ink, { family: CJK, weight: 700 }) +
        rrect(tx, ty, tw, th, r, { fill: PAL.coral, op: 0.92 }) +
        `<circle cx="${tx + tw - r}" cy="${ty + r}" r="${r - 11}" fill="#fff"/>` +
        T(W * 0.5, H * 0.68, 'Settings · Developer · ON', 24, PAL.inkMute, { family: MONO, weight: 400 })
      break
    }
    /* 步骤 3 · 一键汉化（A→中 + 点击涟漪） */
    case 's-localize': {
      const cy = H * 0.43
      scene = T(W * 0.30, cy, 'A', 150, PAL.ink, { weight: 800 }) +
        arrowR(W * 0.42, cy, W * 0.565, PAL.coral, 9) +
        ring(W * 0.70, cy, 98, PAL.coral, '.5', 2) +
        ring(W * 0.70, cy, 122, PAL.coral, '.22', 2) +
        T(W * 0.70, cy, '中', 150, PAL.ink, { family: CJK, weight: 700 }) +
        T(W * 0.5, H * 0.82, '点「一键汉化」', 28, PAL.inkMute, { family: CJK, weight: 500 })
      break
    }
    /* 步骤 4 · 重启成中文（窗口 + 刷新弧） */
    case 's-restart': {
      const wx = W * 0.24, wy = H * 0.27, ww = W * 0.52, wh = H * 0.44
      scene = rrect(wx, wy, ww, wh, 16, { fill: PAL.paper, stroke: PAL.ink, sw: 4 }) +
        `<line x1="${wx}" y1="${wy + 46}" x2="${wx + ww}" y2="${wy + 46}" stroke="${PAL.ink}" stroke-width="3" stroke-opacity=".5"/>` +
        dots(wx + 24, wy + 23) +
        T(wx + ww / 2, wy + 46 + (wh - 46) / 2, '中文', 88, PAL.ink, { family: CJK, weight: 700 }) +
        refreshArc(W * 0.79, H * 0.25, 40, PAL.coral) +
        T(W * 0.5, H * 0.84, 'Claude 自动重启', 26, PAL.inkMute, { family: CJK, weight: 500 })
      break
    }
    default:
      scene = T(W * 0.5, H * 0.5, 'A → 中', 120, PAL.ink, { family: CJK, weight: 700 })
  }

  return `<svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="${fit}" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="ClaudeCN 图示">${bg}${scene}${crosshair(W * 0.85, H * 0.13)}</svg>`
}
