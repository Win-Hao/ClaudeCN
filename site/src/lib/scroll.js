/* 点击 logo 平滑回到页面最顶端（真正的 y=0，含 topbar + 全宽导航），
 * 而不是 #top 锚点（受 scroll-padding-top 影响会停在胶囊位置）。 */
export function scrollToTop(e) {
  if (e) e.preventDefault()
  const reduce = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches
  window.scrollTo({ top: 0, behavior: reduce ? 'auto' : 'smooth' })
}
