import { asset } from '../lib/asset'
import { DOWNLOAD_WIN } from '../lib/links'
import { useLatestDmg } from '../hooks/useLatestDmg'
import StarButton from './StarButton'

/* I · 主视觉 */
export default function Hero() {
  const dmg = useLatestDmg()
  return (
    <section className="hero" id="top" style={{ paddingTop: '40px' }}>
      <div className="container hero-grid">
        <div className="hero-copy">
          <span className="label" data-reveal="">汉化工具 · macOS / Windows</span>
          <h1 className="display" data-reveal="">把 Claude Desktop，<br />变成<em>中文</em>版<span className="dot">.</span></h1>
          <p className="lead" data-reveal="">ClaudeCN 一键把 Claude Desktop 的界面、菜单、原生弹窗全部汉化为简体中文 —— 18,000+ 条精校译文、约 100% 覆盖；不改任何核心功能，随时一键恢复英文原版。</p>
          <div className="hero-actions" data-reveal="" style={{ flexWrap: 'wrap' }}>
            <a className="btn btn-primary" href={dmg} download>下载 macOS 版 <span className="arrow"><svg viewBox="0 0 24 24"><path d="M5 12h14M13 6l6 6-6 6" /></svg></span></a>
            <a className="btn btn-ghost" href={DOWNLOAD_WIN} target="_blank" rel="noopener noreferrer">下载 Windows 版 <span className="arrow"><svg viewBox="0 0 24 24"><path d="M5 12h14M13 6l6 6-6 6" /></svg></span></a>
            <StarButton />
          </div>
          <div className="hero-stats" data-reveal="">
            <span className="stat"><span className="ring coral">中</span><span className="stat-label">18,000+ 译文<b>约 100% 覆盖</b></span></span>
            <span className="stat"><span className="ring">⇄</span><span className="stat-label">一键切换<b>汉化 / 恢复</b></span></span>
            <span className="stat"><span className="ring solid">2</span><span className="stat-label">双平台<b>macOS + Windows</b></span></span>
          </div>
          <div className="hero-foot" data-reveal="">
            <span className="meta">FILED UNDER<br /><b style={{ color: 'var(--ink)' }}>Claude Desktop · 简体中文</b></span>
            <span className="coord">VOL. 01 · MMXXVI · A → 中</span>
          </div>
        </div>
        <div className="hero-art">
          <div className="corner tl" /><div className="corner tr" />
          <div className="corner bl" /><div className="corner br" />
          <span className="annot annot-tl">PLATE Nº 01</span>
          <span className="annot annot-tr coord">检测 → 汉化 → 中文</span>
          <span className="annot annot-bl coord">一键汉化 · 安全可逆</span>
          <img src={asset('assets/app-main.png')} alt="ClaudeCN 主界面：检测到 Claude，一键汉化" loading="eager" />
          <div className="index">
            <span className="on"><span className="n">01</span>检测</span>
            <span><span className="n">02</span>汉化</span>
            <span><span className="n">03</span>重启</span>
            <span><span className="n">04</span>中文</span>
          </div>
        </div>
      </div>
    </section>
  )
}
