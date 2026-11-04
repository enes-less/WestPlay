import { type CSSProperties, useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { type SignedIn, useAskOnce, useSignedIn } from './accounts'
import { appVersion, call, PREVIEW_TUTORIAL } from './bridge'
import { Onboarding } from './onboarding/Onboarding'
import { chosenLanguage, percent, type Words, wordsFor } from './onboarding/words'
import { About } from './pages/About'
import { useHeldKey } from './pages/heldKey'
import { Lesson, TouchLayer } from './pages/Lesson'
import { Launch, TourCard } from './pages/Tour'
import { Accounts } from './pages/Accounts'
import { Controls } from './pages/Controls'
import { Customize } from './pages/Customize'
import { type MapProgress, useMapProgress } from './mapProgress'
import { MapPage } from './pages/MapPage'
import { screenState } from './pages/page'
import { ScreenPage } from './pages/ScreenPage'
import { Setup } from './pages/Setup'
import { Support } from './pages/Support'
import { LanguageSheet } from './pages/LanguageSheet'
import { UninstallSheet } from './pages/UninstallSheet'
import { FinishUpdate, UpdateSheet } from './pages/UpdateSheet'
import { type CompanionWords, companionWords, type SectionId } from './pages/words'
import { RoomText, n, WordsContext } from './room/room'
import './room/room.css'
import { GLOW, type Screen, ScreenContext } from './room/screen'
import { type Banner, GlassCursor, type Pose, TruckScreen } from './room/TruckScreen'
import { useDemo } from './room/useDemo'
import './companion.css'
import { defaultMapAddress, GAME_KINDS, GAME_SHORT_NAMES, type GameCheck, type SetupState, setupState } from './setup'
import { FILES_BASE, useCustomizations } from './customize'
import { type Offer, useUpdateOffer } from './update'
import { type HostStatus, openSocial, type Social, useHostStatus } from './useHostStatus'
import { useKeyName } from './keys'
import { DEFAULT_KEY, type SettingsState, useSettings } from './useSettings'

/**
 * The companion window: its onboarding on the first run (until it's gone through), then the room it ends in.
 * The truck's screen stays in its dark room, turned aside at the right, the live WestPlay on it; the
 * sections are a list at the left, each with what it is now; the page in between. Each section shows on the
 * screen what it's about (its app, its banner, the cursor using it), as the onboarding's steps did.
 */
export function Companion() {
  const status = useHostStatus()
  const settings = useSettings()
  const [setup, setSetup] = useState<SetupState | null>(null)
  const [setupError, setSetupError] = useState<string | null>(null)
  // After the onboarding, its tutorial (from its start, or skipped to its end), before setup's finished.
  const [tutorial, setTutorial] = useState<'lesson' | 'launch' | null>(PREVIEW_TUTORIAL)
  useEffect(() => {
    setupState()
      .then(setSetup)
      .catch((e: unknown) => setSetupError(String(e)))
  }, [])
  useShowWhenReady(setup !== null || setupError !== null)

  if (!setup) return <div className="cp-blank">{setupError ?? ''}</div>
  if (!setup.finished && !tutorial) {
    // The onboarding set the game up: what's set up is read again (the companion read it before), then its
    // tutorial.
    const onboarded = (start: 'lesson' | 'launch') => {
      setupState()
        .then(setSetup, () => {})
        .finally(() => setTutorial(start))
    }
    return <Onboarding initial={setup} status={status} settings={settings} onFinished={onboarded} />
  }
  return (
    <Room
      setup={setup}
      refreshSetup={() => setupState().then(setSetup, () => {})}
      status={status}
      settings={settings}
      tutorial={tutorial}
      tutorialDone={() => {
        setTutorial(null)
        setupState().then(setSetup, () => setSetup({ ...setup, finished: true }))
      }}
    />
  )
}

const SECTIONS: SectionId[][] = [
  ['screen', 'controls', 'customize'],
  ['accounts', 'map'],
  ['setup', 'about'],
  ['support'],
]
/** The sections in order: the tutorial's tour goes through them. */
const TOUR = SECTIONS.flat()

/** The room as it is: the tutorial's lesson (the screen full size), its tour of the sections, its end
 * (what to do in the game), or the companion. */
type Phase = 'lesson' | 'tour' | 'launch' | 'room'

/** Where the screen stands in each. */
const POSE: Record<Phase, Pose> = { lesson: 'full', tour: 'aside', launch: 'final', room: 'aside' }

/** The light the screen casts while each section is up. */
const SECTION_GLOW: Record<SectionId, string> = {
  screen: GLOW.home,
  controls: GLOW.home,
  customize: GLOW.home,
  accounts: GLOW.youtube,
  map: GLOW.maps,
  setup: GLOW.scan,
  about: GLOW.dashboard,
  support: GLOW.home,
}

/** How long a page's words take to leave before the next one's arrive. */
const LEAVE_MS = 220
/** The screen coming out of the onboarding's dive into its place (companion.css `.cp .ts`). */
const ARRIVE_MS = 1300

function Room({
  setup,
  refreshSetup,
  status,
  settings,
  tutorial = null,
  tutorialDone,
}: {
  setup: SetupState
  /** What's set up read again (a game installed into from the Setup page). */
  refreshSetup: () => void
  status: HostStatus | null
  settings: SettingsState
  /** The tutorial, from where it starts; `null`: the companion. */
  tutorial?: 'lesson' | 'launch' | null
  tutorialDone?: () => void
}) {
  const code = chosenLanguage(settings.settings?.language, setup.language)
  const words = wordsFor(code)
  const cw = companionWords(code)
  const [section, setSection] = useState<SectionId>('screen')
  const [leaving, setLeaving] = useState(false)
  const [phase, setPhase] = useState<Phase>(tutorial ?? 'room')
  // The screen comes out of the dive the onboarding ended in, to where the phase has it.
  const [arrivedPose, setArrivedPose] = useState(false)
  const pose: Pose = arrivedPose ? POSE[phase] : 'dive'
  const interactionKey = settings.settings?.interaction.key ?? DEFAULT_KEY
  const held = useHeldKey(interactionKey, settings.settings?.interaction.mode ?? 'hold', phase === 'lesson')
  const demo = useDemo()
  const [powered, setPowered] = useState(false)
  const [banner, setBanner] = useState<Banner | null>(null)
  const [scanning, setScanning] = useState(false)
  const [glow, setGlow] = useState(GLOW.home)
  const [cursor, setCursor] = useState<{ x: number; y: number; pressing: boolean } | null>(null)
  // What a check of each game set up found, for the pages that show it (Setup, Map): checked once, as they open.
  const [checks, setChecks] = useState<Record<string, GameCheck>>({})
  const checked = useCallback((dir: string, check: GameCheck) => setChecks((known) => ({ ...known, [dir]: check })), [])
  // Who's signed in, known at once (what was last known), and asked as the companion opens if never known.
  const signedIn = useSignedIn(status)
  useAskOnce()
  // The uninstall sheet over the room, and the room fading as WestPlay leaves.
  const [uninstalling, setUninstalling] = useState(false)
  // The language sheet (Setup).
  const [choosingLanguage, setChoosingLanguage] = useState(false)
  const [leavingPc, setLeavingPc] = useState(false)
  // An update found, offered in its sheet once per version (Later puts it off; About opens it again).
  const update = useUpdateOffer()
  // The player's wallpapers and icon sets, for Customize and the room's screen.
  const { custom, refresh: refreshCustom } = useCustomizations()
  const [updateShown, setUpdateShown] = useState(false)
  const offered = useRef<string | null>(null)
  const root = useRef<HTMLDivElement>(null)
  const arrived = useRef(performance.now() + ARRIVE_MS)

  // Next frame, so the dive is drawn before it moves on; a window not drawing (minimised) gets there anyway.
  useEffect(() => {
    const frame = requestAnimationFrame(() => setArrivedPose(true))
    const timer = window.setTimeout(() => setArrivedPose(true), 150)
    return () => {
      cancelAnimationFrame(frame)
      window.clearTimeout(timer)
    }
  }, [])

  const { send } = demo
  // The room's screen looks as the truck's does: the player's wallpaper and icons, as chosen.
  const look = settings.settings?.look
  useEffect(() => {
    if (!demo.ready || !custom) return
    send({
      type: 'looks',
      custom,
      base: FILES_BASE,
      wallpaper: look?.wallpaper ?? null,
      iconSet: look?.iconSet ?? null,
      iconStyle: look?.iconStyle ?? 'auto',
    })
  }, [demo.ready, custom, look?.wallpaper, look?.iconSet, look?.iconStyle, send])
  const lastGo = useRef<{ route: string; at: number } | null>(null)
  const screen: Screen = {
    send,
    go: (route) => {
      const now = performance.now()
      if (lastGo.current?.route === route && now - lastGo.current.at < 500) return
      lastGo.current = { route, at: now }
      send({ type: 'go', to: route })
    },
    locate: demo.locate,
    ready: demo.ready,
    banner: setBanner,
    scan: setScanning,
    glow: setGlow,
    cursor: setCursor,
    reboot: () => {
      send({ type: 'power', on: false })
      window.setTimeout(() => send({ type: 'power', on: true }), 450)
    },
    powered,
    leaving,
    settling: () => Math.max(0, arrived.current - performance.now()),
  }

  // On as soon as WestPlay in it is ready: it's been on all along (the player dove into it).
  useEffect(() => {
    if (!demo.ready || powered) return
    send({ type: 'power', on: true })
    setPowered(true)
  }, [demo.ready, powered, send])
  useEffect(() => {
    if (demo.ready) send({ type: 'language', code })
  }, [demo.ready, code, send])
  // The player's map, on the screen's Maps.
  const map = defaultMapAddress(status)
  const mapKey = map ? `${map.base}#${map.built}` : ''
  useEffect(() => {
    if (map) send({ type: 'map', base: map.base, built: map.built })
    // Keyed by the map's address and build.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mapKey, demo.ready, send])

  const open = (next: SectionId) => {
    if (next === section || leaving) return
    setLeaving(true)
    window.setTimeout(() => {
      setSection(next)
      setLeaving(false)
    }, LEAVE_MS)
  }

  const tilt = (event: React.PointerEvent) => {
    const box = root.current?.getBoundingClientRect()
    if (!box) return
    root.current?.style.setProperty('--tilt-x', (((event.clientX - box.left) / box.width) * 2 - 1).toFixed(3))
    root.current?.style.setProperty('--tilt-y', (((event.clientY - box.top) / box.height) * 2 - 1).toFixed(3))
  }

  const nextInTour = () => {
    const at = TOUR.indexOf(section)
    if (at === TOUR.length - 1) setPhase('launch')
    else open(TOUR[at + 1])
  }
  const roomShown = phase === 'tour' || phase === 'room'
  useEffect(() => {
    if (!update || phase !== 'room' || offered.current === update.version) return
    offered.current = update.version
    setUpdateShown(true)
  }, [update, phase])

  // The maps being prepared, worked out once: the Map page's bar and the sidebar show the same.
  const mapProgress = useMapProgress(status)
  const page = {
    status,
    settings,
    setup,
    refreshSetup,
    cw,
    checks,
    mapProgress,
    checked,
    signedIn,
    uninstall: () => setUninstalling(true),
    chooseLanguage: () => setChoosingLanguage(true),
    update,
    showUpdate: () => setUpdateShown(true),
    custom,
    refreshCustom,
    tutorial: () => setPhase('lesson'),
  }
  return (
    <div
      ref={root}
      className="ob cp"
      lang={words.locale}
      data-power={powered || undefined}
      data-leaving-pc={leavingPc || undefined}
      data-phase={phase}
      style={{ '--section-glow': SECTION_GLOW[section] } as CSSProperties}
      onPointerMove={tilt}
    >
      <TruckScreen frame={demo.frame} ready={demo.ready} pose={pose} glow={glow} banner={banner} scanning={scanning}>
        <GlassCursor
          x={cursor?.x ?? 640}
          y={cursor?.y ?? 420}
          pressing={cursor?.pressing ?? false}
          visible={cursor !== null}
        />
        {phase === 'lesson' && <TouchLayer active={held} press={(at) => send({ type: 'press', ...at })} />}
      </TruckScreen>
      <WordsContext.Provider value={{ words, code }}>
        <RoomText.Provider value={{ locale: words.locale, back: words.back }}>
          <ScreenContext.Provider value={screen}>
            {phase === 'lesson' && (
              <Lesson
                cw={cw}
                interactionKey={interactionKey}
                held={held}
                route={demo.route}
                next={() => {
                  open('screen')
                  setPhase('tour')
                }}
              />
            )}
            {phase === 'launch' && (
              <Launch
                done={() => {
                  setPhase('room')
                  tutorialDone?.()
                }}
              />
            )}
            {roomShown && (
              <SectionList
                section={section}
                open={open}
                cw={cw}
                words={words}
                status={status}
                signedIn={signedIn}
                settings={settings}
                setup={setup}
                update={update}
                mapProgress={mapProgress}
              />
            )}
            {roomShown && section === 'screen' && <ScreenPage {...page} />}
            {roomShown && section === 'controls' && <Controls {...page} />}
            {roomShown && section === 'customize' && <Customize {...page} />}
            {roomShown && section === 'accounts' && <Accounts {...page} />}
            {roomShown && section === 'map' && <MapPage {...page} />}
            {roomShown && section === 'setup' && <Setup {...page} />}
            {roomShown && section === 'about' && <About {...page} />}
            {roomShown && section === 'support' && <Support {...page} />}
            {phase === 'tour' && (
              <TourCard cw={cw} section={section} last={section === TOUR[TOUR.length - 1]} next={nextInTour} />
            )}
            {choosingLanguage && (
              <LanguageSheet
                cw={cw}
                settings={settings}
                game={chosenLanguage(null, setup.language)}
                close={() => setChoosingLanguage(false)}
              />
            )}
            {uninstalling && (
              <UninstallSheet cw={cw} close={() => setUninstalling(false)} farewell={() => setLeavingPc(true)} />
            )}
            {updateShown && update && !uninstalling && (
              <UpdateSheet cw={cw} offer={update} close={() => setUpdateShown(false)} />
            )}
            {phase === 'room' && <FinishUpdate cw={cw} setup={setup} />}
          </ScreenContext.Provider>
        </RoomText.Provider>
      </WordsContext.Provider>
    </div>
  )
}

/**
 * The sections, at the left: WestPlay's name and where it is now, then each section with what it is now
 * (WestPlay on or off, the key, who's signed in, the map, the game's version, WestPlay's own or an update), in
 * groups. The one that's open is lit by a capsule of glass that glides from section to section.
 */
function SectionList({
  section,
  open,
  cw,
  words,
  status,
  signedIn,
  settings,
  setup,
  update,
  mapProgress,
}: {
  section: SectionId
  open(section: SectionId): void
  cw: CompanionWords
  words: Words
  status: HostStatus | null
  signedIn: SignedIn
  settings: SettingsState
  setup: SetupState
  update: Offer | null
  mapProgress: MapProgress
}) {
  const list = useRef<HTMLElement>(null)
  const [lit, setLit] = useState<{ top: number; height: number } | null>(null)
  const version = useVersion()
  const state = screenState(status)
  const keyName = useKeyName(settings.settings?.interaction.key ?? DEFAULT_KEY, words)

  useLayoutEffect(() => {
    const row = list.current?.querySelector<HTMLElement>(`[data-section="${section}"]`)
    if (row) setLit({ top: row.offsetTop, height: row.offsetHeight })
  }, [section])

  const value: Record<SectionId, string> = {
    screen:
      state === 'live'
        ? cw.values.on
        : state === 'original'
          ? cw.values.off
          : state === 'notInCab'
            ? cw.values.waiting
            : state === 'unsupported'
              ? cw.screen.screenUnsupported
              : '',
    controls: keyName,
    customize: '',
    accounts:
      signedIn.youtube === null && signedIn.spotify === null && signedIn.appleMusic === null
        ? ''
        : [signedIn.youtube && 'YouTube', signedIn.spotify && 'Spotify', signedIn.appleMusic && 'Apple Music']
            .filter(Boolean)
            .join(', ') || cw.values.none,
    map: mapValue(status, mapProgress, cw, words),
    // The games set up, by their short names (their versions too long for the list).
    setup: GAME_KINDS.filter((kind) => setup.games.some((game) => game.kind === kind))
      .map((kind) => GAME_SHORT_NAMES[kind])
      .join(' & '),
    about: update ? cw.update.badge : version,
    support: '',
  }

  let place = 2
  return (
    <nav ref={list} className="cp-list" aria-label="WestPlay">
      <header className="cp-brand ob-in" style={n(0)}>
        <img src="/westplay.png" alt="" />
        <span>
          <b>WestPlay</b>
          <span className="cp-state" data-state={state}>
            <i />
            {cw.state[state]}
          </span>
        </span>
      </header>
      {lit && <i className="cp-lit" style={{ top: lit.top, height: lit.height }} />}
      {SECTIONS.map((group, g) => (
        <div key={g} className="cp-group">
          {group.map((id) => (
            <button
              key={id}
              type="button"
              className="cp-row ob-in"
              style={n(place++)}
              data-section={id}
              data-here={id === section || undefined}
              onClick={() => open(id)}
            >
              <b>{cw.sections[id]}</b>
              <span>{value[id]}</span>
            </button>
          ))}
        </div>
      ))}
      <footer className="cp-socials ob-in" style={n(place++)}>
        {SOCIALS.map(({ site, name, glyph }) => (
          <button
            key={site}
            type="button"
            data-site={site}
            title={name}
            aria-label={name}
            onClick={() => {
              openSocial(site).catch((error: unknown) => console.error(`${name} could not be opened`, error))
            }}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d={glyph} />
            </svg>
          </button>
        ))}
      </footer>
    </nav>
  )
}

/** WestPlay's pages elsewhere, at the foot of the list: each by its own name and mark (Simple Icons' glyphs, CC0). */
const SOCIALS: { site: Social; name: string; glyph: string }[] = [
  {
    site: 'youtube',
    name: 'YouTube',
    glyph:
      'M23.498 6.186a3.016 3.016 0 0 0-2.122-2.136C19.505 3.545 12 3.545 12 3.545s-7.505 0-9.377.505A3.017 3.017 0 0 0 .502 6.186C0 8.07 0 12 0 12s0 3.93.502 5.814a3.016 3.016 0 0 0 2.122 2.136c1.871.505 9.376.505 9.376.505s7.505 0 9.377-.505a3.015 3.015 0 0 0 2.122-2.136C24 15.93 24 12 24 12s0-3.93-.502-5.814zM9.545 15.568V8.432L15.818 12l-6.273 3.568z',
  },
  {
    site: 'discord',
    name: 'Discord',
    glyph:
      'M20.317 4.37a19.791 19.791 0 0 0-4.885-1.515.074.074 0 0 0-.079.037c-.21.375-.444.865-.608 1.25a18.27 18.27 0 0 0-5.487 0 12.64 12.64 0 0 0-.617-1.25.077.077 0 0 0-.079-.037A19.736 19.736 0 0 0 3.677 4.37a.07.07 0 0 0-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 0 0 .031.057 19.9 19.9 0 0 0 5.993 3.03.078.078 0 0 0 .084-.028c.462-.63.874-1.295 1.226-1.994a.076.076 0 0 0-.041-.106 13.107 13.107 0 0 1-1.872-.892.077.077 0 0 1-.008-.128 10.2 10.2 0 0 0 .372-.292.074.074 0 0 1 .077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 0 1 .078.01c.12.098.246.198.373.292a.077.077 0 0 1-.006.127 12.299 12.299 0 0 1-1.873.892.077.077 0 0 0-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 0 0 .084.028 19.839 19.839 0 0 0 6.002-3.03.077.077 0 0 0 .032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 0 0-.031-.03zM8.02 15.33c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.956-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.946 2.418-2.157 2.418z',
  },
  {
    site: 'patreon',
    name: 'Patreon',
    glyph:
      'M22.957 7.21c-.004-3.064-2.391-5.576-5.191-6.482-3.478-1.125-8.064-.962-11.384.604C2.357 3.231 1.093 7.391 1.046 11.54c-.039 3.411.302 12.396 5.369 12.46 3.765.047 4.326-4.804 6.068-7.141 1.24-1.662 2.836-2.132 4.801-2.618 3.376-.836 5.678-3.501 5.673-7.031Z',
  },
]

function mapValue(status: HostStatus | null, progress: MapProgress, cw: CompanionWords, words: Words): string {
  const map = status?.map
  switch (map?.state) {
    case 'ready':
    case 'building':
      // The maps being made (one after another): how far, the Map page's bar's number.
      return progress.building ? percent(words, progress.shown) : cw.values.ready
    case 'waitingForGame':
    case 'waitingForMemory':
      return cw.values.waiting
    case 'failed':
    case 'notAllowed':
      return cw.values.failed
    default:
      return ''
  }
}

/** WestPlay's version, once the app has said. */
function useVersion(): string {
  const [version, setVersion] = useState('')
  useEffect(() => {
    appVersion().then(setVersion, () => setVersion(''))
  }, [])
  return version
}

/** The window starts hidden (so it never shows blank while this page loads): it shows once `ready`, after the
 * page has painted. */
function useShowWhenReady(ready: boolean) {
  useEffect(() => {
    if (!ready) return
    // Two frames: the first screen is laid out, then painted.
    let frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => {
        call('companion_ready').then(
          // It's out now: the room goes as it comes up, not a second later (room/useDemo.ts).
          () => window.dispatchEvent(new Event('focus')),
          (error: unknown) => console.error('The window could not be shown', error),
        )
      })
    })
    return () => cancelAnimationFrame(frame)
  }, [ready])
}
