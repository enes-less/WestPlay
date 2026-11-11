import { type CSSProperties, useEffect, useRef, useState } from 'react'
import { isInHost } from '../../../shared/host'
import { whenTransitionEnds } from '../../state/launch'
import { useLook } from '../../state/hooks'
import { useCabin } from '../../state/store'
import { useText } from '../../state/text'
import { followArea } from '../youtube/youtube'
import { rectInArea, usePageHole } from './hole'
import { pictureOf, send, useSafari, zoomOf } from './safari'

/** How long the CarPlay area takes at most to grow back from the truck's screen, ms (its spring). */
const GROW_MS = 900

/** A little longer than the cab's frames take to reach the screen, ms, before the page under it is hidden. */
const HIDE_AFTER_MS = 50

/** Runs `then` once the cab has drawn two frames more (and `ms` after): what it changed is on the screen by then. */
function afterFrames(then: () => void, ms = 0): void {
  requestAnimationFrame(() => requestAnimationFrame(() => (ms > 0 ? setTimeout(then, ms) : then())))
}

/** How long a tab's page takes to come in at most (its animation, safari.css), ms. */
const ENTER_MS = 420

/** How far inside the page the cab is cut open, px: its panel overlapping the page's edge. */
const EDGE_OVERLAP = 1

/** The page's zoom on top of the cab's scale: a web page's text as large as the cab's own (a page's body text
 * is 16 px, the cab's 21). */
const PAGE_ZOOM = 1.3

/**
 * Where tab `tab`'s page shows: the host places the page under the cab at this element, and the CarPlay area is cut
 * open over it (Shell.tsx), the cab's covers over it left (hole.ts): the keyboard rises over it, as over everything
 * (owner 2026-11-08), the page scrolling the field typed into into sight (safari.js). The page isn't part of
 * the cab, so it can't move with it: while Safari's window grows or shrinks, or its screen comes or goes, the
 * tab's picture stands in, and the page takes over where it stops. Outside WestPlay (a plain browser) the
 * picture is all there is.
 */
export function WebPage({ tab, url }: { tab: number; url: string }) {
  const frame = useRef<HTMLDivElement>(null)
  const handedOver = useCabin((state) => state.truckScreen)
  // The cab's appearance, the pages' too (owner 2026-11-08: every website follows the cab's light and dark).
  const dark = useLook().appearance === 'dark'
  const darkNow = useRef(dark)
  darkNow.current = dark
  // The cab's language: the pages are asked for in it, as an iPad's Safari asks in the device's (owner 2026-11-08).
  const { locale } = useText()
  const localeNow = useRef(locale)
  localeNow.current = locale
  const covers = usePageHole((state) => state.covers)
  const zoom = useSafari((state) => zoomOf(state, url))
  const picture = useSafari((state) => pictureOf(tab, state.pictures[tab]))
  // The address to load the page from, if the host has let it go: read as it's placed, never a reason to place.
  const address = useRef(url)
  address.current = url
  // Handed over before this placing: the CarPlay area is growing back.
  const returning = useRef(false)
  // Placing it now, while it follows the screen (none while it doesn't): what changes its rectangle without
  // moving the screen (the keyboard, a cover, the zoom) places it again with this.
  const placeNow = useRef<(() => void) | null>(null)
  const pageZoom = useRef(zoom)
  pageZoom.current = zoom
  // The cab cut open over the page only once both: its own area has come in (it animates in as a tab opens or is
  // picked), and the page has drawn something (a new WebView draws nothing at all till then, not even white: cut open
  // over it, the game's own screen showed through, owner 2026-11-08). Till then the cab's own page stands in.
  const painted = useSafari((state) => !isInHost() || !!state.pages[tab]?.painted)
  // The page's own ground, as the page says: the cab's pixel round the page (it's cut open a pixel inside the page's
  // edge) and what stands in for the page are in it, never white round a dark page (owner 2026-11-09).
  const ground = useSafari((state) => state.pages[tab]?.ground)
  const [entered, setEntered] = useState(false)
  const ready = useRef(false)
  ready.current = painted && entered
  useEffect(() => {
    // At the latest after its animation's time, whatever told of it.
    const timer = setTimeout(() => setEntered(true), ENTER_MS)
    return () => clearTimeout(timer)
  }, [])

  useEffect(() => {
    const element = frame.current
    if (!element) return
    if (handedOver) {
      returning.current = true
      return
    }
    const slot = element.closest('.screen-slot')
    const moving = () => !!slot && (slot.classList.contains('entering') || slot.classList.contains('leaving'))
    const place = () => {
      const hole = rectInArea(element)
      const area = document.querySelector('.carplay')?.getBoundingClientRect()
      if (!hole || !area) return
      // The cut a pixel inside the page all round: the cab's panel over the page's edge, not meeting it there (two
      // edges smoothed against each other let a hairline of the page through, owner 2026-11-08). The page itself is
      // cut to its own rectangle and round corners by the host (`radius`), so none of it shows past the cab's.
      const cut = {
        x: hole.x + EDGE_OVERLAP,
        y: hole.y + EDGE_OVERLAP,
        width: hole.width - 2 * EDGE_OVERLAP,
        height: hole.height - 2 * EDGE_OVERLAP,
        radius: Math.max(0, hole.radius - EDGE_OVERLAP),
        ...(hole.top !== undefined ? { top: Math.max(0, hole.top - EDGE_OVERLAP) } : {}),
      }
      // The cab cut open only once the page is there under it, drawn (the host shows it as it's told, below): cut
      // first, the cab let the game's own screen through (owner 2026-11-08: the game's navigation flashing).
      if (ready.current) {
        afterFrames(() => {
          if (!gone && ready.current) usePageHole.setState({ hole: cut })
        })
      }
      if (!isInHost()) return
      const scale = window.devicePixelRatio
      const cabZoom = element.getBoundingClientRect().width / (element.offsetWidth || 1)
      const toFrame = (rect: { x: number; y: number; width: number; height: number }) => ({
        x: Math.round((area.left + rect.x) * scale),
        y: Math.round((area.top + rect.y) * scale),
        width: Math.round(rect.width * scale),
        height: Math.round(rect.height * scale),
      })
      const page = toFrame(hole)
      send({
        action: 'show',
        tab,
        url: address.current,
        ...page,
        zoom: Math.round(cabZoom * PAGE_ZOOM * pageZoom.current * 1000) / 1000,
        covers: Object.values(usePageHole.getState().covers).map(toFrame),
        dark: darkNow.current,
        radius: Math.round(hole.radius * scale * 100) / 100,
        language: localeNow.current,
      })
    }
    let following: (() => void) | null = null
    let settling: ReturnType<typeof setTimeout>[] = []
    let gone = false
    const follow = () => {
      if (gone || following || moving()) return
      following = followArea(place, element)
      placeNow.current = place
      // Safari's window grows in from its icon, the keyboard slides up (transforms, which no size change tells
      // of): placed again as they settle.
      settling = [120, 350, 700].map((ms) => setTimeout(place, ms))
    }
    const stopped = slot ? new MutationObserver(follow) : null
    if (slot) stopped?.observe(slot, { attributes: true, attributeFilter: ['class'] })
    const area = document.querySelector('.carplay')
    let grown: ReturnType<typeof setTimeout> | undefined
    const grew = (event: Event) => {
      if (event.target !== area || (event as TransitionEvent).propertyName !== 'transform') return
      area?.removeEventListener('transitionend', grew)
      clearTimeout(grown)
      follow()
    }
    if (returning.current && area) {
      returning.current = false
      area.addEventListener('transitionend', grew)
      grown = setTimeout(() => {
        area.removeEventListener('transitionend', grew)
        follow()
      }, GROW_MS)
    } else {
      returning.current = false
      whenTransitionEnds(follow)
    }
    return () => {
      gone = true
      stopped?.disconnect()
      area?.removeEventListener('transitionend', grew)
      clearTimeout(grown)
      settling.forEach(clearTimeout)
      placeNow.current = null
      if (!following) return
      following()
      usePageHole.setState({ hole: null })
      // Its picture, for the tab overview and for standing in as Safari comes back.
      send({ action: 'picture', tab })
      // The page hidden only once the cab has drawn itself whole over it again: hidden at once, the cab still cut open
      // there for a frame, the game's own screen showed through (owner 2026-11-08).
      afterFrames(() => send({ action: 'hide', tab }), HIDE_AFTER_MS)
    }
  }, [tab, handedOver])

  // A cover (the keyboard over the page, a menu), the zoom: told again; the page itself stays where it is. Come in and
  // drawn: cut open over it.
  useEffect(() => placeNow.current?.(), [covers, zoom, painted, entered])
  // The cab turning light or dark: the pages with it, at once.
  useEffect(() => send({ action: 'appearance', dark }), [dark])

  return (
    <div
      ref={frame}
      className="sf-page entering"
      style={ground ? ({ '--sf-page-ground': ground } as CSSProperties) : undefined}
      onAnimationEnd={(event) => {
        if (event.target === event.currentTarget) setEntered(true)
      }}
    >
      {picture && <img className="sf-page-picture" src={picture} alt="" />}
    </div>
  )
}
