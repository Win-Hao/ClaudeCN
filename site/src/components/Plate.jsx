import { buildPlate } from '../lib/plates.js'

/* 矢量 SVG 插画位（流程四步用） */
export default function Plate({ kind, fit = 'slice' }) {
  return <div data-plate={kind} dangerouslySetInnerHTML={{ __html: buildPlate(kind, { fit }) }} />
}
