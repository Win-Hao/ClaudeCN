import { useReveals } from './hooks/useReveals.js'
import SideRails from './components/SideRails.jsx'
import Topbar from './components/Topbar.jsx'
import Nav from './components/Nav.jsx'
import Hero from './components/Hero.jsx'
import Features from './components/Features.jsx'
import Steps from './components/Steps.jsx'
import CTA from './components/CTA.jsx'
import Footer from './components/Footer.jsx'

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
