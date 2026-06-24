import { buildPlate } from '../lib/plates'

interface PlateProps {
  kind: string
  fit?: 'slice' | 'meet'
}

/* 矢量 SVG 插画位 */
export default function Plate({ kind, fit = 'slice' }: PlateProps) {
  return <div data-plate={kind} dangerouslySetInnerHTML={{ __html: buildPlate(kind, { fit }) }} />
}
