import { useEffect, useState } from 'react'

/* Headroom 式粘性导航状态：
 *  - hidden  : 下滑且越过 threshold 时隐藏，上滑时出现
 *  - scrolled: 离开页面顶部后为 true（用于切换磨砂吸顶态）
 * 用 rAF 节流滚动事件，passive 监听。返回 { hidden, scrolled }。 */
export function useHeadroom(threshold = 240, stuckAt = 8) {
  const [state, setState] = useState({ hidden: false, scrolled: false })

  useEffect(() => {
    let lastY = window.pageYOffset
    let ticking = false

    const onScroll = () => {
      const y = window.pageYOffset
      const hidden = y > lastY && y > threshold
      const scrolled = y > stuckAt
      setState((prev) =>
        prev.hidden === hidden && prev.scrolled === scrolled ? prev : { hidden, scrolled },
      )
      lastY = y
      ticking = false
    }
    const handler = () => {
      if (!ticking) { window.requestAnimationFrame(onScroll); ticking = true }
    }
    window.addEventListener('scroll', handler, { passive: true })
    onScroll() // 初始化（刷新时可能已不在顶部）
    return () => window.removeEventListener('scroll', handler)
  }, [threshold, stuckAt])

  return state
}
