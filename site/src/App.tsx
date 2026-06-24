import { useReveals } from './hooks/useReveals'
import SideRails from './components/SideRails'
import Topbar from './components/Topbar'
import Nav from './components/Nav'
import Hero from './components/Hero'
import Features from './components/Features'
import Steps from './components/Steps'
import CTA from './components/CTA'
import Footer from './components/Footer'

export default function App() {
  // 挂载后接管所有 [data-reveal] 元素的滚动揭示（GSAP，含降级）
  useReveals()

  return (
    <div className="shell">
      <SideRails />
      <Topbar />
      <Nav />
      <Hero />
      <Features />
      <Steps />
      <CTA />
      <Footer />
    </div>
  )
}
