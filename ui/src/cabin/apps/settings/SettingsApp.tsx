import { type CSSProperties, type ReactNode, useEffect, useState } from 'react'
import { useWallpapers } from '../../state/looks'
import { pickVariant, prepareWallpaper } from '../../wallpapers'
import { type Appearance, resolveAppearance } from '../../state/appearance'
import { useLook, useScreenProfile } from '../../state/hooks'
import { DASHBOARD_WIDGETS, type DashboardWidget, useCabin, useLive } from '../../state/store'
import { useText } from '../../state/text'
import { AppearanceGlyph, CheckGlyph, ChevronGlyph, DashboardGlyph, NowPlayingGlyph, WallpaperGlyph } from './glyphs'
import { type SettingsPage, useSettings } from './settings'
import './settings.css'

/**
 * The Settings app, in iOS 26's Settings look: a large title, inset grouped cards, coloured tiles, chevrons
 * and pill switches, in Apple's colours (shell/apple.css). Only the player's own CarPlay choices live here;
 * WestPlay's management (the cab's sound, the companion's options) belongs to the companion window.
 *
 * Appearance, Wallpaper, the Dashboard's widgets and Now Playing's album art; text size comes later.
 */
export function SettingsApp() {
  const text = useText()
  const page = useSettings((state) => state.page)
  const went = useSettings((state) => state.went)
  // Opened again, the page it was left on comes with the window, without a slide of its own.
  useEffect(() => () => useSettings.setState({ went: 'none' }), [])
  return (
    <section className="app-panel settings-app" aria-label={text.apps.settings}>
      <div key={page} className={`settings-page went-${went}`}>
        <PageContent page={page} />
      </div>
    </section>
  )
}

function PageContent({ page }: { page: SettingsPage }) {
  switch (page) {
    case 'list':
      return <ListPage />
    case 'appearance':
      return <AppearancePage />
    case 'wallpaper':
      return <WallpaperPage />
    case 'dashboard':
      return <DashboardPage />
    case 'nowPlaying':
      return <NowPlayingPage />
  }
}

/** The appearance the mode asks for now (before a wallpaper with one variant only overrules it). */
function useWantedAppearance(): Appearance {
  const mode = useCabin((state) => state.appearanceMode)
  const gameTime = useLive((state) => state.telemetry?.gameTime ?? null)
  const lowBeam = useLive((state) => state.telemetry?.lowBeam ?? false)
  return resolveAppearance(mode, { gameTime, lowBeam })
}

function ListPage() {
  const text = useText()
  const open = useSettings((state) => state.open)
  const mode = useCabin((state) => state.appearanceMode)
  const wanted = useWantedAppearance()
  return (
    <Page title={text.apps.settings} list>
      <Group>
        <NavRow
          tile="var(--apple-blue)"
          icon={<AppearanceGlyph />}
          label={text.settings.appearance}
          value={mode === 'auto' ? text.settings.automatic : text.settings[wanted]}
          onOpen={() => open('appearance')}
        />
        <NavRow
          tile="var(--apple-cyan)"
          icon={<WallpaperGlyph />}
          label={text.settings.wallpaper}
          onOpen={() => open('wallpaper')}
        />
      </Group>
      <Group>
        <NavRow
          tile="var(--apple-indigo)"
          icon={<DashboardGlyph />}
          label={text.settings.dashboard}
          onOpen={() => open('dashboard')}
        />
        <NavRow
          tile="var(--apple-pink)"
          icon={<NowPlayingGlyph />}
          label={text.nowPlaying}
          onOpen={() => open('nowPlaying')}
        />
      </Group>
    </Page>
  )
}

/** Light or Dark, each a little picture of the screen, and Automatic (iOS's Display & Brightness). */
function AppearancePage() {
  const text = useText()
  const mode = useCabin((state) => state.appearanceMode)
  const setMode = useCabin((state) => state.setAppearanceMode)
  const wanted = useWantedAppearance()
  // The Vehicle Bar: only on a screen that has one (the truck's own bar, beside WestPlay).
  const hasBar = useScreenProfile().oemStrip !== null
  const oemMode = useCabin((state) => state.oemMode)
  const chooseOemMode = useCabin((state) => state.chooseOemMode)
  return (
    <Page title={text.settings.appearance}>
      <Group footer={text.settings.automaticFooter}>
        <div className="appearance-choices">
          {(['light', 'dark'] as const).map((appearance) => (
            <button
              key={appearance}
              type="button"
              className="appearance-choice"
              aria-pressed={wanted === appearance}
              onClick={() => setMode(appearance)}
            >
              <ScreenPicture appearance={appearance} />
              <span className="appearance-name">{text.settings[appearance]}</span>
              <span className={wanted === appearance ? 'settings-radio on' : 'settings-radio'}>
                {wanted === appearance && <CheckGlyph />}
              </span>
            </button>
          ))}
        </div>
        <SwitchRow
          label={text.settings.automatic}
          on={mode === 'auto'}
          // Off, it keeps what Automatic shows now.
          onToggle={(on) => setMode(on ? 'auto' : wanted)}
        />
      </Group>
      {hasBar && (
        <Group footer={text.settings.vehicleBarFooter}>
          <SwitchRow
            label={text.settings.vehicleBar}
            on={oemMode === 'auto'}
            onToggle={(on) => chooseOemMode(on ? 'auto' : 'off')}
          />
        </Group>
      )}
    </Page>
  )
}

/** A little picture of the screen in an appearance: a wallpaper, the sidebar and the Dashboard's widgets. */
function ScreenPicture({ appearance }: { appearance: Appearance }) {
  return (
    <span className={`screen-picture ${appearance}`} aria-hidden>
      <span className="screen-picture-sidebar" />
      <span className="screen-picture-widget large" />
      <span className="screen-picture-widget" />
      <span className="screen-picture-widget" />
    </span>
  )
}

function WallpaperPage() {
  const text = useText()
  const wallpaperId = useCabin((state) => state.wallpaperId)
  const setWallpaper = useCabin((state) => state.setWallpaper)
  const wallpapers = useWallpapers()
  const { appearance } = useLook()
  return (
    <Page title={text.settings.wallpaper}>
      <Group>
        <div className="wallpaper-choices">
          {wallpapers.map((wallpaper) => (
            <button
              key={wallpaper.id}
              type="button"
              className="wallpaper-choice"
              aria-pressed={wallpaper.id === wallpaperId}
              onClick={() => setWallpaper(wallpaper.id)}
            >
              <WallpaperThumbnail src={pickVariant(wallpaper, appearance).src} />
              <span className="wallpaper-name">{wallpaper.name}</span>
            </button>
          ))}
        </div>
      </Group>
    </Page>
  )
}

/** Thumbnails, each drawn once from its wallpaper: the files are up to 4000 px wide. */
const thumbnails = new Map<string, Promise<string>>()

function WallpaperThumbnail({ src }: { src: string }) {
  const [thumbnail, setThumbnail] = useState<string | null>(null)
  useEffect(() => {
    let current = true
    let made = thumbnails.get(src)
    if (!made) {
      made = prepareWallpaper(src, THUMBNAIL_WIDTH, (THUMBNAIL_WIDTH * 9) / 16).catch(() => src)
      thumbnails.set(src, made)
    }
    void made.then((url) => current && setThumbnail(url))
    return () => {
      current = false
    }
  }, [src])
  return thumbnail ? <img src={thumbnail} alt="" /> : <span className="wallpaper-thumbnail-empty" />
}

/** Twice a thumbnail's width on the screen, so it stays sharp. */
const THUMBNAIL_WIDTH = 600

/** Which widgets the Dashboard shows (turn-by-turn always does while there's a route). */
function DashboardPage() {
  const text = useText()
  const widgets = useCabin((state) => state.dashboardWidgets)
  const setWidget = useCabin((state) => state.setDashboardWidget)
  const names: Record<DashboardWidget, string> = {
    nowPlaying: text.nowPlaying,
    destination: text.destination,
    weather: text.apps.weather,
  }
  return (
    <Page title={text.settings.dashboard}>
      <Group header={text.settings.widgets}>
        {DASHBOARD_WIDGETS.map((widget) => (
          <SwitchRow key={widget} label={names[widget]} on={widgets[widget]} onToggle={(on) => setWidget(widget, on)} />
        ))}
      </Group>
    </Page>
  )
}

/** The album art on Now Playing and in Music. */
function NowPlayingPage() {
  const text = useText()
  const albumArt = useCabin((state) => state.albumArt)
  const setAlbumArt = useCabin((state) => state.setAlbumArt)
  return (
    <Page title={text.nowPlaying}>
      <Group>
        <SwitchRow label={text.settings.showAlbumArt} on={albumArt} onToggle={setAlbumArt} />
      </Group>
    </Page>
  )
}

/**
 * A page: its content scrolling under a bar that holds the back button and the page's title. On the list,
 * the title is large at the top of the content, and the bar's small one takes over once it scrolls under.
 * Content scrolled under the bar fades and blurs away, iOS 26's edge.
 */
function Page({ title, list = false, children }: { title: string; list?: boolean; children: ReactNode }) {
  const text = useText()
  const back = useSettings((state) => state.back)
  const [scrolled, setScrolled] = useState(false)
  const [largeTitleGone, setLargeTitleGone] = useState(false)
  return (
    <>
      <div
        className={list ? 'settings-scroll list' : 'settings-scroll'}
        onScroll={(event) => {
          const top = event.currentTarget.scrollTop
          setScrolled(top > 0)
          setLargeTitleGone(top > LARGE_TITLE_HEIGHT)
        }}
      >
        {list && <h1 className="settings-large-title">{title}</h1>}
        {children}
      </div>
      <header className={scrolled ? 'settings-bar scrolled' : 'settings-bar'}>
        {!list && (
          <button type="button" className="settings-back" aria-label={text.settings.back} onClick={back}>
            <ChevronGlyph back />
          </button>
        )}
        <div className={!list || largeTitleGone ? 'settings-bar-title shown' : 'settings-bar-title'}>{title}</div>
      </header>
    </>
  )
}

/** How far the list scrolls before its large title is under the bar (settings.css: .settings-large-title). */
const LARGE_TITLE_HEIGHT = 56

function Group({ header, footer, children }: { header?: string; footer?: string; children: ReactNode }) {
  return (
    <div className="settings-group">
      {header && <h2 className="settings-group-header">{header}</h2>}
      <div className="settings-card">{children}</div>
      {footer && <p className="settings-group-footer">{footer}</p>}
    </div>
  )
}

interface RowLabel {
  label: string
  /** A coloured tile with a white glyph before the label (rows on the list have one). */
  tile?: string
  icon?: ReactNode
}

function RowStart({ label, tile, icon }: RowLabel) {
  return (
    <>
      {tile && (
        <span className="settings-tile" style={{ background: tile } as CSSProperties}>
          {icon}
        </span>
      )}
      <span className="settings-label">{label}</span>
    </>
  )
}

/** A row that opens a page: its value in grey, and a chevron. */
function NavRow({ value, onOpen, ...start }: RowLabel & { value?: string; onOpen: () => void }) {
  return (
    <button type="button" className={start.tile ? 'settings-row with-tile' : 'settings-row'} onClick={onOpen}>
      <RowStart {...start} />
      {value && <span className="settings-value">{value}</span>}
      <span className="settings-chevron">
        <ChevronGlyph />
      </span>
    </button>
  )
}

/** A row with a switch; the whole row flips it (a big target on a truck's screen). */
function SwitchRow({ on, onToggle, ...start }: RowLabel & { on: boolean; onToggle: (on: boolean) => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      className={start.tile ? 'settings-row with-tile' : 'settings-row'}
      onClick={() => onToggle(!on)}
    >
      <RowStart {...start} />
      <span className={on ? 'apple-switch on' : 'apple-switch'} aria-hidden>
        <span className="apple-switch-thumb" />
      </span>
    </button>
  )
}
