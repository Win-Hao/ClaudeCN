import { useEffect, useState } from 'react'
import { DOWNLOAD_MAC } from '../lib/links'

// 取「最新 release 里的 .dmg」直链，让 macOS 下载按钮永远指向最新版、无需每次发版改代码。
// 初始用 links.ts 里的兜底直链（本身就是个可用的直接下载），拉到后再升级为 API 返回的最新值；
// 请求失败 / 限流 / 最新 release 没有 dmg 时，保持兜底值。
const API = 'https://api.github.com/repos/Win-Hao/ClaudeCN/releases/latest'

interface ReleaseAsset {
  name: string
  browser_download_url: string
}
interface LatestRelease {
  assets?: ReleaseAsset[]
}

export function useLatestDmg(): string {
  const [url, setUrl] = useState<string>(DOWNLOAD_MAC)

  useEffect(() => {
    let alive = true
    fetch(API)
      .then((r) => (r.ok ? r.json() : null))
      .then((d: LatestRelease | null) => {
        if (!alive || !d || !Array.isArray(d.assets)) return
        const dmg = d.assets.find((a) => /\.dmg$/i.test(a.name))
        if (dmg && dmg.browser_download_url) setUrl(dmg.browser_download_url)
      })
      .catch(() => {})
    return () => {
      alive = false
    }
  }, [])

  return url
}
