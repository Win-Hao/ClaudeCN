import BrandMark from './BrandMark.jsx'
import { useHeadroom } from '../hooks/useHeadroom.js'
import { scrollToTop } from '../lib/scroll.js'
import { GITHUB_URL, RELEASES_URL } from '../lib/links.js'

const NAV_LINKS = [
  { href: '#features', label: '功能', num: '01' },
  { href: '#steps', label: '流程', num: '02' },
]

/* 粘性导航：始终吸顶；滚过顶部后切成胶囊并保持固定 */
export default function Nav() {
  const { scrolled } = useHeadroom()
  const cls = 'nav' + (scrolled ? ' is-stuck' : '')
  return (
    <header className={cls} id="nav">
      <div className="container">
        <div className="nav-inner">
          <a className="brand" href="#top" onClick={scrollToTop}>
            <BrandMark />
            ClaudeCN
            <span className="brand-meta">Claude 汉化<b>中文界面</b></span>
          </a>
          <ul className="nav-links">
            {NAV_LINKS.map((l) => (
              <li key={l.href}><a href={l.href}>{l.label}<span className="num">{l.num}</span></a></li>
            ))}
          </ul>
          <div className="nav-side">
            <a className="nav-github" href={GITHUB_URL} target="_blank" rel="noopener noreferrer" aria-label="在 GitHub 上查看 ClaudeCN">
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path fill="currentColor" d="M12 .5C5.37.5 0 5.78 0 12.29c0 5.2 3.44 9.6 8.21 11.16.6.11.82-.25.82-.56 0-.28-.01-1.02-.02-2-3.34.71-4.04-1.58-4.04-1.58-.55-1.37-1.34-1.74-1.34-1.74-1.09-.73.08-.72.08-.72 1.21.08 1.84 1.22 1.84 1.22 1.07 1.8 2.81 1.28 3.5.98.11-.76.42-1.28.76-1.57-2.67-.3-5.47-1.31-5.47-5.83 0-1.29.47-2.34 1.24-3.17-.13-.3-.54-1.52.12-3.16 0 0 1.01-.32 3.3 1.21a11.6 11.6 0 0 1 3-.4c1.02 0 2.05.13 3 .4 2.29-1.53 3.3-1.21 3.3-1.21.66 1.64.25 2.86.12 3.16.77.83 1.24 1.88 1.24 3.17 0 4.53-2.81 5.52-5.49 5.81.43.37.81 1.1.81 2.22 0 1.6-.01 2.9-.01 3.29 0 .31.21.68.83.56A12.04 12.04 0 0 0 24 12.29C24 5.78 18.63.5 12 .5z" />
              </svg>
            </a>
            <a className="nav-cta" href={RELEASES_URL} target="_blank" rel="noopener noreferrer">下载</a>
          </div>
        </div>
      </div>
    </header>
  )
}
