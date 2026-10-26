import { useLayoutEffect, useRef, useState } from 'react'
import { useAppIcon } from '../state/hooks'
import { type AppId, APPS, RELEASED_APP_IDS } from '../state/apps'
import { useLaunchStyle } from '../state/launch'
import { go, useCabin } from '../state/store'
import { useText } from '../state/text'
import { textScale } from '../state/textSize'
import { CELL, COMPACT_CELL, LABEL_LINES, pagesOf, pageSize, rowLayout } from './pages'
import './home.css'

/**
 * CarPlay-style Home: the app grid next to the sidebar, in pages as CarPlay's: as many columns and rows as the
 * screen fits (pages.ts), the next page a drag sideways away (shell/dragScroll.ts), with a dot for each page
 * below.
 */
export function Home() {
  const pagesElement = useRef<HTMLDivElement>(null)
  const [size, setSize] = useState({ columns: 4, rows: 2, compact: false, height: 0 })
  const [page, setPage] = useState(0)
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
  const turnTo = (index: number) => {
    const element = pagesElement.current
    element?.scrollTo({ left: index * element.clientWidth, behavior: 'smooth' })
  }

  return (
    <section className="screen home" aria-label="Home">
      <div
        ref={pagesElement}
        className={size.compact ? 'home-pages compact' : 'home-pages'}
        onScroll={(event) => {
          const element = event.currentTarget
          setPage(Math.round(element.scrollLeft / Math.max(1, element.clientWidth)))
        }}
      >
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
      {pages.length > 1 && (
        <div className="home-dots">
          {pages.map((_, index) => (
            <button
              key={index}
              type="button"
              className={index === page ? 'home-dot shown' : 'home-dot'}
              aria-label={`${index + 1} / ${pages.length}`}
              aria-current={index === page}
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
