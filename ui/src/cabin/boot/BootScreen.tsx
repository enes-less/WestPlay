import { type CSSProperties, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { postToHost } from '../../shared/host'
import type { PowerPhase } from '../state/power'
import { useLive } from '../state/store'
import { logoBrands } from './brand'
import './boot.css'

/**
 * Full-screen boot visuals for the power phases before `on` (see state/power.ts):
 * - `splash`: the truck's own boot screen, its brand logo on black.
 * - `connecting`: the WestPlay intro: the W mark appears, then "WestPlay" unrolls out of it to the right.
 * - `off`: black (the game hides the screen anyway while the electrics are off).
 */
export function BootScreen({ phase }: { phase: Exclude<PowerPhase, 'on'> }) {
  return (
    <div className={`boot boot-${phase}`} aria-hidden>
      {phase === 'splash' && <BrandSplash />}
      {phase === 'connecting' && <WestPlayIntro />}
    </div>
  )
}

interface LogoSource {
  src: string
  /** Shown in its own colours; otherwise drawn as a white silhouette, like most truck boot screens. */
  color: boolean
}

/**
 * Files tried in order for a truck's logo, named by brand id (its own, then the game's brand a modded truck
 * is a version of: `logoBrands`): `scania.color.png` keeps its colours (for logos whose detail is colour,
 * like Scania's griffin); `daf.svg` or `daf.png` is drawn white.
 */
function logoSources(brandId: string, brand: string): LogoSource[] {
  return logoBrands(brandId, brand).flatMap((id) => [
    { src: `/brand-logos/${id}.color.svg`, color: true },
    { src: `/brand-logos/${id}.color.png`, color: true },
    { src: `/brand-logos/${id}.svg`, color: false },
    { src: `/brand-logos/${id}.png`, color: false },
  ])
}

/**
 * The truck brand's logo. Logos are their makers' trademarks, so they live outside the repository
 * (`public/brand-logos`, git-ignored); when a brand has none, its name is shown as a wordmark instead.
 */
function BrandSplash() {
  const truck = useLive((state) => state.truck)
  const [attempt, setAttempt] = useState(0)
  // For the host's startup log: the moment the logo is on the page.
  useEffect(() => {
    if (truck) postToHost({ type: 'startup', stage: 'truck logo shown' })
  }, [truck])
  if (!truck) return null

  const sources = logoSources(truck.brandId, truck.brand)
  return (
    <div className="boot-brand">
      {attempt < sources.length ? (
        <img
          key={sources[attempt].src}
          className={sources[attempt].color ? 'boot-brand-logo color' : 'boot-brand-logo'}
          src={sources[attempt].src}
          alt=""
          onError={() => setAttempt((current) => current + 1)}
        />
      ) : (
        <span className="boot-brand-wordmark">{truck.brand}</span>
      )}
    </div>
  )
}

/**
 * The W mark scales in at the centre, then glides left while "WestPlay" is revealed out of it to the
 * right, ending as a centred pair. The mark's starting offset is half the width the text adds, measured
 * before the first paint (and again once the web font has loaded), so the mark starts exactly at the
 * centre whatever the font renders.
 */
function WestPlayIntro() {
  const wordmark = useRef<HTMLSpanElement>(null)
  const [shift, setShift] = useState(0)

  useLayoutEffect(() => {
    const element = wordmark.current
    if (!element) return
    const measure = () => {
      const gap = parseFloat(getComputedStyle(element.parentElement!).columnGap) || 0
      setShift((element.offsetWidth + gap) / 2)
    }
    measure()
    // Measure again once the web font is in: a fallback font has a different width.
    let active = true
    document.fonts.ready.then(() => active && measure())
    return () => {
      active = false
    }
  }, [])

  return (
    <div className="boot-intro" style={{ '--mark-shift': `${shift}px` } as CSSProperties}>
      <span className="boot-intro-mark">
        <img src="/westplay.png" alt="" />
      </span>
      <span ref={wordmark} className="boot-intro-wordmark">
        WestPlay
      </span>
    </div>
  )
}
