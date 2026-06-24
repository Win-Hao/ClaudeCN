import { useEffect } from 'react'
import { gsap } from 'gsap'
import { ScrollTrigger } from 'gsap/ScrollTrigger'

gsap.registerPlugin(ScrollTrigger)

/* 滚动揭示：GSAP ScrollTrigger 批处理 + 错落 stagger，一次性 once。
 * 降级：prefers-reduced-motion 或异常 → 直接全部显示，绝不白屏。
 * 在 App 挂载后调用一次；它扫描整个文档的 [data-reveal] 元素。 */
export function useReveals(): void {
  useEffect(() => {
    const reveals = Array.from(document.querySelectorAll<HTMLElement>('[data-reveal]'))
    if (!reveals.length) return

    const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches
    const revealInstant = () => reveals.forEach((el) => el.setAttribute('data-revealed', 'true'))

    if (reduceMotion) {
      revealInstant()
      return
    }

    let triggers: ScrollTrigger[] = []
    try {
      document.documentElement.classList.add('gsap-reveals')

      reveals.forEach((el) => {
        const v = el.getAttribute('data-reveal')
        const from: gsap.TweenVars = { autoAlpha: 0 }
        if (v === 'left') from.x = -36
        else if (v === 'right') from.x = 36
        else if (v === 'scale') from.scale = 0.96
        else if (v === 'rise-lg') { from.y = 44; from.scale = 0.985 }
        else from.y = 28
        gsap.set(el, from)
      })

      const revealNow = (els: HTMLElement[]) => {
        if (!els.length) return
        gsap.set(els, { autoAlpha: 1, x: 0, y: 0, scale: 1 })
        els.forEach((el) => { el.style.willChange = 'auto' })
      }
      const revealAnim = (els: HTMLElement[]) => {
        if (!els.length) return
        gsap.to(els, {
          autoAlpha: 1, x: 0, y: 0, scale: 1,
          duration: 0.7, ease: 'power3.out', stagger: 0.08, overwrite: 'auto',
          onComplete: () => els.forEach((el) => { el.style.willChange = 'auto' }),
        })
      }
      const makeBatch = (els: HTMLElement[]) =>
        ScrollTrigger.batch(els, { start: 'top 88%', once: true, onEnter: (b: Element[]) => revealAnim(b as HTMLElement[]) })

      // 等浏览器恢复滚动位置后再分流（rAF 保证在 scroll restoration 之后执行）。
      const initRaf = requestAnimationFrame(() => {
        const y = window.scrollY || window.pageYOffset || 0
        if (y < 4) {
          makeBatch(reveals)
        } else {
          const vh = window.innerHeight
          const settled: HTMLElement[] = []
          const rest: HTMLElement[] = []
          reveals.forEach((el) => { (el.getBoundingClientRect().top < vh ? settled : rest).push(el) })
          revealNow(settled)
          makeBatch(rest)
        }
        triggers = ScrollTrigger.getAll()
        ScrollTrigger.refresh()
      })

      // 页面高度随图片/字体加载变化 → 防抖校正触发点
      let refreshRaf = 0
      const refreshSoon = () => {
        if (refreshRaf) return
        refreshRaf = requestAnimationFrame(() => { refreshRaf = 0; ScrollTrigger.refresh() })
      }
      const imgs = Array.from(document.images)
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
    } catch {
      document.documentElement.classList.remove('gsap-reveals')
      reveals.forEach((el) => {
        el.style.opacity = ''
        el.style.visibility = ''
        el.style.transform = ''
        el.style.willChange = ''
      })
      revealInstant()
    }
  }, [])
}
