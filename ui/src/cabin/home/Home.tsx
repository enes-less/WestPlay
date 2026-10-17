import { useLayoutEffect, useRef, useState } from 'react'
import { useAppIcon } from '../state/hooks'
import { type AppId, APPS, RELEASED_APP_IDS } from '../state/apps'
import { useLaunchStyle } from '../state/launch'
import { go } from '../state/store'
import { useText } from '../state/text'
import { APP_HEIGHT, pagesOf, pageSize } from './pages'
import './home.css'

/**
 * CarPlay-style Home: the app grid next to the sidebar, in pages as CarPlay's: as many columns and rows as the
 * screen fits (pages.ts), the next page a drag sideways away (shell/dragScroll.ts), with a dot for each page
 * below.
 */
export function Home() {
  const pagesElement = useRef<HTMLDivElement>(null)
  const [size, setSize] = useState({ columns: 4, rows: 2 })
  const [page, setPage] = useState(0)

  useLayoutEffect(() => {
    const element = pagesElement.current
    if (!element) return
    const measure = () => {
      const next = pageSize(element.clientWidth, element.clientHeight)
      setSize((current) => (current.columns === next.columns && current.rows === next.rows ? current : next))
    }
    measure()
    const resized = new ResizeObserver(measure)
    resized.observe(element)
    return () => resized.disconnect()
  }, [])

  const pages = pagesOf(RELEASED_APP_IDS, size.columns * size.rows)
  const turnTo = (index: number) => {
    const element = pagesElement.current
    element?.scrollTo({ left: index * element.clientWidth, behavior: 'smooth' })
  }

  return (
    <section className="screen home" aria-label="Home">
      <div
        ref={pagesElement}
        className="home-pages"
        onScroll={(event) => {
          const element = event.currentTarget
          setPage(Math.round(element.scrollLeft / Math.max(1, element.clientWidth)))
        }}
      >
        {pages.map((apps, index) => (
          <div
            key={index}
            className="home-grid"
            // Every page has the page's whole grid: a page with fewer apps has them where the first has its.
            style={{
              gridTemplateColumns: `repeat(${size.columns}, minmax(0, 1fr))`,
              gridTemplateRows: `repeat(${size.rows}, ${APP_HEIGHT}px)`,
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
