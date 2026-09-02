const ArrowMark = () => (
  <span className="arrow-mark"><svg viewBox="0 0 24 24"><path d="M7 17L17 7M9 7h8v8" /></svg></span>
)

interface Card {
  num: string
  tag: string
  title: string
  body: string
}

const CARDS: Card[] = [
  { num: '01', tag: '汉化', title: '一键汉化', body: '界面、菜单、原生弹窗一次性全覆盖，点一下即可。' },
  { num: '02', tag: '恢复', title: '一键还原', body: '完整备份英文原版，随时一键恢复，安全无损。' },
  { num: '03', tag: '覆盖', title: '约 100%', body: '29,000+ 条精校译文，未译的新词自动回退英文。' },
  { num: '04', tag: '安全', title: '不改核心', body: '只改界面文字，不动功能代码，不收集任何数据。' },
]

/* I · 核心功能（4 卡横排） */
export default function Features() {
  return (
    <section id="features">
      <div className="container">
        <div className="sec-rule">
          <span className="roman">I.</span>
          <span className="meta-grp"><span>核心功能</span><span className="dot-mark">·</span><span>一键搞定，安全可逆</span></span>
          <span>01 / 03</span>
        </div>
        <div className="section-header">
          <span className="label" data-reveal="">核心功能<span className="ix">I</span></span>
          <h2 className="display" data-reveal="">省心，也<em>省事</em><span className="dot">.</span></h2>
        </div>
        <div className="features-grid">
          {CARDS.map((c) => (
            <div className="card" data-reveal="" key={c.num}>
              <div className="num">{c.num}<span className="tag">{c.tag}</span></div>
              <h3>{c.title}</h3>
              <p>{c.body}</p>
              <ArrowMark />
            </div>
          ))}
        </div>
      </div>
    </section>
  )
}
