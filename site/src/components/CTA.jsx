import Plate from './Plate.jsx'
import { DOWNLOAD_WIN } from '../lib/links.js'
import { useLatestDmg } from '../hooks/useLatestDmg.js'

/* III · 下载 CTA */
export default function CTA() {
  const dmg = useLatestDmg()
  return (
    <section className="cta" id="cta">
      <div className="container">
        <div className="sec-rule">
          <span className="roman">III.</span>
          <span className="meta-grp"><span>开始使用</span><span className="dot-mark">·</span><span>下载 ClaudeCN</span></span>
          <span>03 / 03</span>
        </div>
        <div className="cta-grid">
          <div>
            <span className="label" data-reveal="">开始使用<span className="ix">III</span></span>
            <h2 className="display" data-reveal="">现在就<br />换成<em>中文</em><span className="dot">.</span></h2>
            <p className="lead" data-reveal="">下载 ClaudeCN，点一下「一键汉化」，下次打开 Claude 就是中文界面。</p>
            <div className="cta-actions" data-reveal="">
              <a className="btn btn-primary" href={dmg} download>下载 macOS 版 <span className="arrow"><svg viewBox="0 0 24 24"><path d="M12 4v12M6 12l6 6 6-6" /></svg></span></a>
              <a className="email-pill" href={DOWNLOAD_WIN} target="_blank" rel="noopener noreferrer">下载 Windows 版 <span className="arrow-circle">↗</span></a>
            </div>
            <p className="cta-note" data-reveal="">完全免费（CC BY-NC 4.0）。macOS 版已 Apple 公证，双击即开；Windows 版请在 Releases 页手动下载、需管理员权限。严禁任何形式的商业倒卖。</p>
            <div className="cta-foot" data-reveal="">
              <span className="stamp">完全免费</span>
              <span>macOS 13+ · Windows 10/11</span>
              <span>by Winhao学AI</span>
            </div>
          </div>
          <div className="cta-art" data-reveal="right">
            <span className="ribbon">FIN. · MMXXVI</span>
            <span className="index">中</span>
            <Plate kind="p-done" />
          </div>
        </div>
      </div>
    </section>
  )
}
