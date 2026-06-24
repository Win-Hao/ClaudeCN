interface Step {
  num: string
  title: string
  desc: string
}

const STEPS: Step[] = [
  { num: '01', title: '安装', desc: '下载并打开 ClaudeCN（macOS 拖进「应用程序」；Windows 直接运行）。' },
  { num: '02', title: '开发者模式', desc: '在 Claude 里 Settings → Developer 打开开关，避免被服务端改回英文。' },
  { num: '03', title: '一键汉化', desc: '回到 ClaudeCN 点「一键汉化」，按提示授权后自动替换。' },
  { num: '04', title: '重启成中文', desc: 'Claude 自动重启变中文；想还原点「恢复英文原版」即可。' },
]

/* II · 使用流程（四步横排，无重型堆叠） */
export default function Steps() {
  return (
    <section id="steps">
      <div className="container">
        <div className="sec-rule">
          <span className="roman">II.</span>
          <span className="meta-grp"><span>使用流程</span><span className="dot-mark">·</span><span>四步，英文变中文</span></span>
          <span>02 / 03</span>
        </div>
        <div className="section-header">
          <span className="label" data-reveal="">使用流程<span className="ix">II</span></span>
          <h2 className="display" data-reveal="">四步，<br />英文变<em>中文</em><span className="dot">.</span></h2>
        </div>
        <div className="method-grid">
          {STEPS.map((s) => (
            <div className="method-step" data-reveal="" key={s.num}>
              <div className="num">{s.num}</div>
              <h4>{s.title}<span className="arrow-r">→</span></h4>
              <p>{s.desc}</p>
            </div>
          ))}
        </div>
        <div className="method-foot">
          <span className="left"><span className="ring" />本地完成 · 安全可逆</span>
          <span className="right">全程在本地，<b>不上传任何数据</b></span>
        </div>
      </div>
    </section>
  )
}
