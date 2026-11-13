import { type PointerEvent as ReactPointerEvent, useLayoutEffect, useRef, useState } from 'react'
import { useAppIcon } from '../state/hooks'
import { type AppId, APPS, RELEASED_APP_IDS } from '../state/apps'
import { useLaunchStyle } from '../state/launch'
import { go, useCabin } from '../state/store'
import { useText } from '../state/text'
import { textScale } from '../state/textSize'
import { DRAG_FROM, releaseSpeed } from '../shell/dragScroll'
import { CELL, COMPACT_CELL, EDGE_GIVE, LABEL_LINES, pagesOf, pageSize, rowLayout, settlePage } from './pages'
import './home.css'

/**
 * CarPlay-style Home: the app grid next to the sidebar, in pages as CarPlay's: as many columns and rows as the
 * screen fits (pages.ts), the next page a drag sideways away, with a dot for each page below. The pages are one track
 * moved by transform: it follows the drag, and as it's let go it always moves on to the page it settles on
 * (`settlePage`), from wherever it was let go (owner 2026-11-11: let go 60% of the way, the pages "just snap"; scroll
 * snapping, turned on again as a drag ended, jumped to the page before the smooth scroll began).
 */
export function Home() {
  const pagesElement = useRef<HTMLDivElement>(null)
  const [size, setSize] = useState({ columns: 4, rows: 2, compact: false, height: 0 })
  const [page, setPage] = useState(0)
  const track = useRef<HTMLDivElement>(null)
  // A press on the pages, and once it's moved sideways, the drag: where it began, its moves (time, x), and the screen
  // pixels to a page pixel (the cab is drawn scaled to its screen).
  const press = useRef<{
    pointer: number
    x: number
    y: number
    dragging: boolean
    samples: [number, number][]
    ratio: number
  } | null>(null)
  // The labels grow with the player's Text Size, and the cells with them.
  const scale = textScale(useCabin((state) => state.textSize))
  const names = useText().apps
  // Each app's label's lines as drawn (a long name takes two): the rows are as tall as their names need.
  const [lines, setLines] = useState<Partial<Record<AppId, number>>>({})

  useLayoutEffect(() => {
    const element = pagesElement.current
    if (!element) return
    const measure = () => {
      const next = { ...pageSize(element.clientWidth, element.clientHeight, scale), height: element.clientHeight }
      setSize((current) =>
        current.columns === next.columns &&
        current.rows === next.rows &&
        current.compact === next.compact &&
        current.height === next.height
          ? current
          : next,
      )
    }
    measure()
    const resized = new ResizeObserver(measure)
    resized.observe(element)
    return () => resized.disconnect()
  }, [scale])

  // The names' lines, at the columns' width: again as the columns, the text or its size change, and once the web
  // font is in (a fallback font's names wrap elsewhere).
  useLayoutEffect(() => {
    const element = pagesElement.current
    if (!element) return
    const count = () => {
      const found: Partial<Record<AppId, number>> = {}
      for (const label of element.querySelectorAll<HTMLElement>('.app .label')) {
        const app = label.parentElement?.dataset.app as AppId | undefined
        if (!app) continue
        const style = getComputedStyle(label)
        const line = parseFloat(style.lineHeight)
        const text = label.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom)
        found[app] = line > 0 ? Math.min(LABEL_LINES, Math.max(1, Math.round(text / line))) : 1
      }
      setLines((current) => (RELEASED_APP_IDS.every((app) => current[app] === found[app]) ? current : found))
    }
    count()
    let active = true
    document.fonts.ready.then(() => active && count())
    return () => {
      active = false
    }
  }, [size.columns, size.compact, scale, names])

  const cell = size.compact ? COMPACT_CELL : CELL
  const rows = rowLayout(
    cell,
    scale,
    size.height,
    size.columns,
    size.rows,
    RELEASED_APP_IDS.map((app) => lines[app] ?? 1),
  )
  const pages = pagesOf(RELEASED_APP_IDS, size.columns * rows.rows)
  const last = pages.length - 1
  const shown = Math.min(page, last)

  /** The track at page `at`, moved by `offset` (page pixels): animated (`animate`) or at once. */
  const place = (at: number, offset: number, animate: boolean) => {
    const element = track.current
    if (!element) return
    element.classList.toggle('dragging', !animate)
    element.style.transform = `translateX(calc(${-at * 100}% + ${offset}px))`
  }
  // At the page shown as it changes (a dot pressed, fewer pages), moving there.
  useLayoutEffect(() => place(shown, 0, true), [shown])

  const turnTo = (index: number) => {
    setPage(index)
    place(index, 0, true)
  }

  const down = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return
    const element = event.currentTarget
    const ratio = element.clientWidth > 0 ? element.getBoundingClientRect().width / element.clientWidth : 1
    press.current = {
      pointer: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      dragging: false,
      samples: [],
      ratio: ratio > 0 ? ratio : 1,
    }
  }
  const move = (event: ReactPointerEvent<HTMLDivElement>) => {
    const pressed = press.current
    if (!pressed || pressed.pointer !== event.pointerId) return
    // The button already up: its release never came (owner 2026-11-13, in the cab: dragged to the screen's edge and
    // let go there, the pages went on following the mouse). Let go here.
    if ((event.buttons & 1) === 0) {
      up(event)
      return
    }
    const dx = (event.clientX - pressed.x) / pressed.ratio
    if (!pressed.dragging) {
      const dy = (event.clientY - pressed.y) / pressed.ratio
      if (Math.hypot(dx, dy) < DRAG_FROM) return
      if (Math.abs(dy) > Math.abs(dx)) {
        press.current = null
        return
      }
      pressed.dragging = true
      event.currentTarget.setPointerCapture(event.pointerId)
    }
    pressed.samples.push([event.timeStamp, event.clientX / pressed.ratio])
    if (pressed.samples.length > 32) pressed.samples.shift()
    // Past the first or the last page, it gives.
    const beyond = (shown === 0 && dx > 0) || (shown === last && dx < 0)
    place(shown, beyond ? dx * EDGE_GIVE : dx, false)
  }
  const up = (event: ReactPointerEvent<HTMLDivElement>) => {
    const pressed = press.current
    if (!pressed || pressed.pointer !== event.pointerId) return
    press.current = null
    if (!pressed.dragging) return
    // What it's let go over isn't pressed: the drag was the gesture.
    const swallow = (click: Event) => {
      click.stopPropagation()
      click.preventDefault()
    }
    window.addEventListener('click', swallow, { capture: true, once: true })
    setTimeout(() => window.removeEventListener('click', swallow, { capture: true }), 0)
    // Where it's let go counts: held still before it, it's no flick (the moves alone were quick). A cancel or a lost
    // capture says nowhere: where it last moved to.
    const here = event.type === 'pointerup' || event.type === 'pointermove'
    const x = here ? event.clientX / pressed.ratio : (pressed.samples.at(-1)?.[1] ?? pressed.x / pressed.ratio)
    if (here) pressed.samples.push([event.timeStamp, x])
    const width = pagesElement.current?.clientWidth ?? 1
    const offset = x - pressed.x / pressed.ratio
    const target = settlePage(shown, pages.length, offset, width, releaseSpeed(pressed.samples))
    setPage(target)
    place(target, 0, true)
  }

  return (
    <section className="screen home" aria-label="Home">
      <div
        ref={pagesElement}
        className={size.compact ? 'home-pages compact' : 'home-pages'}
        // Its own drag (the page's drag to scroll leaves it: shell/dragScroll.ts).
        data-own-drag
        onPointerDown={down}
        onPointerMove={move}
        onPointerUp={up}
        onPointerCancel={up}
        // The pointer taken from it without a release (its capture lost): let go where it was last.
        onLostPointerCapture={up}
      >
        <div ref={track} className="home-track">
          {pages.map((apps, index) => (
            <div
              key={index}
              className="home-grid"
              // Every page has the page's whole grid: a page with fewer apps has them where the first has its. The rows
              // the first page fills, not all that fit: seven apps in two rows centred, not at the top of three (owner
              // 2026-10-24: "icons are too close to the top").
              style={{
                gridTemplateColumns: `repeat(${size.columns}, minmax(0, 1fr))`,
                gridTemplateRows: rows.heights.map((height) => `${height}px`).join(' '),
                rowGap: rows.gap,
              }}
            >
              {apps.map((id) => (
                <HomeApp key={id} app={id} />
              ))}
            </div>
          ))}
        </div>
      </div>
      {pages.length > 1 && (
        <div className="home-dots">
          {pages.map((_, index) => (
            <button
              key={index}
              type="button"
              className={index === shown ? 'home-dot shown' : 'home-dot'}
              aria-label={`${index + 1} / ${pages.length}`}
              aria-current={index === shown}
              onClick={() => turnTo(index)}
            />
          ))}
        </div>
      )}
    </section>
  )
}

/** An app on Home. Its icon is what the app grows out of when it opens, and shrinks back into. */
function HomeApp({ app }: { app: AppId }) {
  const launch = useLaunchStyle(app, { kind: 'icon', origin: 'home' })
  const text = useText()
  const icon = useAppIcon(APPS[app].icon)
  return (
    <button type="button" className="app" data-app={app} onClick={() => go({ type: 'openApp', app }, 'home')}>
      <img src={icon} alt="" style={launch} />
      <span className="label">{text.apps[app]}</span>
    </button>
  )
}
