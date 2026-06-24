import BrandMark from './BrandMark'
import { scrollToTop } from '../lib/scroll'
import { GITHUB_URL, RELEASES_URL, CHANGELOG_URL, ISSUES_URL, PROFILE_URL, LICENSE_URL } from '../lib/links'

interface FootCol {
  h: string
  links: Array<[string, string]>
}

const COLS: FootCol[] = [
  { h: '产品', links: [['核心功能', '#features'], ['使用流程', '#steps'], ['下载', '#cta']] },
  { h: '资源', links: [['下载最新版', RELEASES_URL], ['更新日志', CHANGELOG_URL], ['常见问题', ISSUES_URL], ['源代码', GITHUB_URL]] },
  { h: '项目', links: [['提交反馈', ISSUES_URL], ['作者主页', PROFILE_URL]] },
  { h: '说明', links: [['安全与备份', GITHUB_URL], ['开源协议 · CC BY-NC', LICENSE_URL]] },
]

export default function Footer() {
  return (
    <footer>
      <div className="container">
        <div className="foot-grid">
          <div className="foot-brand">
            <a className="brand" href="#top" onClick={scrollToTop}><BrandMark />ClaudeCN</a>
            <p>把 Claude Desktop 一键汉化为简体中文，支持 macOS 与 Windows；不改核心功能，随时可恢复。由 <a className="inline-link" href={PROFILE_URL} target="_blank" rel="noopener noreferrer">Winhao学AI</a>（抖音 54927876676）打造。</p>
          </div>
          {COLS.map((col) => (
            <div className="foot-col" key={col.h}>
              <h5>{col.h}</h5>
              <ul>
                {col.links.map(([label, href]) => {
                  const external = href.startsWith('http')
                  return (
                    <li key={label}>
                      <a href={href} {...(external ? { target: '_blank', rel: 'noopener noreferrer' } : {})}>{label}</a>
                    </li>
                  )
                })}
              </ul>
            </div>
          ))}
        </div>
        <div className="foot-bottom">
          <span><span className="pulse" />© MMXXVI ClaudeCN · Claude Desktop 汉化</span>
          <span className="right">
            <span>macOS</span>
            <span>Windows</span>
            <span>免费开源</span>
          </span>
        </div>
      </div>
      <div className="foot-mega">
        <div className="container wide"><div className="word">ClaudeCN<em>.</em></div></div>
      </div>
    </footer>
  )
}
