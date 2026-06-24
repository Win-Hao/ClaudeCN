import { useEffect } from 'react'
import { gsap } from 'gsap'
import { ScrollTrigger } from 'gsap/ScrollTrigger'

gsap.registerPlugin(ScrollTrigger)

/* 滚动揭示：GSAP ScrollTrigger 批处理 + 错落 stagger，一次性 once。
 * 守住 gsap-performance 准则（只动 transform/opacity、用完释放 will-change）。
 * 降级：prefers-reduced-motion 或异常 → 直接全部显示，绝不白屏。
 *
 * 在 App 挂载后调用一次；它扫描整个文档的 [data-reveal] 元素。 */
export function useReveals() {
  useEffect(() => {
    const reveals = Array.prototype.slice.call(document.querySelectorAll('[data-reveal]'))
    if (!reveals.length) return

    const reduceMotion = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches
    const revealInstant = () => reveals.forEach((el) => el.setAttribute('data-revealed', 'true'))

    if (reduceMotion) {
      revealInstant()
      return
    }

    let triggers = []
    try {
      document.documentElement.classList.add('gsap-reveals')

      reveals.forEach((el) => {
        const v = el.getAttribute('data-reveal')
        const from = { autoAlpha: 0 }
        if (v === 'left') from.x = -36
        else if (v === 'right') from.x = 36
        else if (v === 'scale') from.scale = 0.96
        else if (v === 'rise-lg') { from.y = 44; from.scale = 0.985 }
        else from.y = 28
        gsap.set(el, from)
      })

      // 瞬显（不进 stagger 队列）：刷新时已经滚过去 / 当前屏可见的元素直接呈现
      const revealNow = (els) => {
        if (!els.length) return
        gsap.set(els, { autoAlpha: 1, x: 0, y: 0, scale: 1 })
        els.forEach((el) => { el.style.willChange = 'auto' })
      }
      // 错落揭示：之后滚动进入视口的元素逐个淡入
      const revealAnim = (els) => {
        if (!els.length) return
        gsap.to(els, {
          autoAlpha: 1, x: 0, y: 0, scale: 1,
          duration: 0.7, ease: 'power3.out', stagger: 0.08, overwrite: 'auto',
          onComplete: () => els.forEach((el) => { el.style.willChange = 'auto' }),
        })
      }
      const makeBatch = (els) =>
        ScrollTrigger.batch(els, { start: 'top 88%', once: true, onEnter: (b) => revealAnim(b) })

      // 等浏览器恢复滚动位置后再分流（rAF 保证在 scroll restoration 之后执行）。
      // 痛点：刷新停在页面中部时，ScrollTrigger.batch 会把「视口上方所有已越线元素」收成一个
      // 大组、用 stagger 串行播放，当前屏要干等上方一长串（还看不见的）动画播完才轮到。于是：
      //   · 从顶部正常加载（scrollY≈0）→ 全部交给 batch，保留逐屏入场揭示；
      //   · 刷新恢复到中部 → 当前屏及以上直接瞬显（不进 stagger），只有下方元素滚到时才揭示。
      const initRaf = requestAnimationFrame(() => {
        const y = window.scrollY || window.pageYOffset || 0
        if (y < 4) {
          makeBatch(reveals)
        } else {
          const vh = window.innerHeight
          const settled = []
          const rest = []
          reveals.forEach((el) => { (el.getBoundingClientRect().top < vh ? settled : rest).push(el) })
          revealNow(settled)
          makeBatch(rest)
        }
        triggers = ScrollTrigger.getAll()
        ScrollTrigger.refresh()
      })

      // 页面高度随图片/字体加载变化 → 防抖校正触发点（别只等 window.load，本页图 ~5MB）
      let refreshRaf = 0
      const refreshSoon = () => {
        if (refreshRaf) return
        refreshRaf = requestAnimationFrame(() => { refreshRaf = 0; ScrollTrigger.refresh() })
      }
      const imgs = Array.prototype.slice.call(document.images)
      if (document.fonts && document.fonts.ready) document.fonts.ready.then(refreshSoon)
      imgs.forEach((img) => { if (!img.complete) img.addEventListener('load', refreshSoon) })
      window.addEventListener('load', refreshSoon)

      return () => {
        cancelAnimationFrame(initRaf)
        if (refreshRaf) cancelAnimationFrame(refreshRaf)
        window.removeEventListener('load', refreshSoon)
        imgs.forEach((img) => img.removeEventListener('load', refreshSoon))
        triggers.forEach((t) => t.kill())
        document.documentElement.classList.remove('gsap-reveals')
      }
    } catch (err) {
      document.documentElement.classList.remove('gsap-reveals')
      reveals.forEach((el) => {
        el.style.opacity = ''; el.style.visibility = ''; el.style.transform = ''; el.style.willChange = ''
      })
      revealInstant()
    }
  }, [])
}
