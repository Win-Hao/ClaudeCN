import { asset } from '../lib/asset'

/* 品牌标记：App 图标（A→中，圆角方形） */
export default function BrandMark() {
  return (
    <span className="brand-mark">
      <img src={asset('assets/logo.png')} alt="ClaudeCN" />
    </span>
  )
}
