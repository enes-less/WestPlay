import { useContext, useEffect, useRef, useState } from 'react'
import { hvtMapMessage } from '../../cabin/maps/blockedMap'
import {
  checkGame,
  defaultMapAddress,
  chooseGameFolder,
  type GameCheck,
  type GameInfo,
  type InstallReport,
  installIntoGame,
  type SetupState,
} from '../setup'
import {
  type HostStatus,
  appleMusicCheck,
  appleMusicSignIn,
  appleMusicSignOut,
  spotifyCheck,
  spotifySignIn,
  spotifySignOut,
  youtubeCheck,
  youtubeSignIn,
  youtubeSignOut,
} from '../useHostStatus'
import { DEFAULT_KEY, type SettingsState } from '../useSettings'
import { memoryNote } from '../pages/page'
import { companionWords } from '../pages/words'
import '../room/room.css'
import { type Banner, GlassCursor, type Pose, TruckScreen } from '../room/TruckScreen'
import {
  Button,
  Checking,
  Cross,
  Group,
  KeyChooser,
  MapProgress,
  Note,
  Panel,
  ProfileRow,
  Row,
  Status,
  Tick,
} from '../room/parts'
import {
  banner,
  explain,
  gameSummary,
  n,
  RoomText,
  useChecking,
  useGameRunning,
  useGlidingProgress,
  useSignOut,
  useWords,
  WordsContext,
} from '../room/room'
import {
  GLOW,
  ON_SCREEN,
  openAndClose,
  type Screen,
  ScreenContext,
  useScreen,
  useScreenAt,
  useTour,
} from '../room/screen'
import { useDemo } from '../room/useDemo'
import { count, languageOf, list, percent, wordsFor } from './words'

const STEPS = [
  'welcome',
  'game',
  'check',
  'install',
  'map',
  'controls',
  'accounts',
  'spotify',
  'appleMusic',
  'tutorial',
] as const
type StepId = (typeof STEPS)[number]

const POSES: Record<StepId, Pose> = {
  welcome: 'hero',
  game: 'side',
  check: 'side',
  install: 'side',
  map: 'side',
  controls: 'side',
  accounts: 'side',
  spotify: 'side',
  appleMusic: 'side',
  tutorial: 'final',
}

/** How long the words take to leave before the next step's arrive. */
const LEAVE_MS = 240
/** The WestPlay intro on the screen, from power on to Home (cabin/state/power.ts CONNECTING_MS, and a
 * beat). */
const BOOT_MS = 2900
/** How long the screen takes to move to another place (onboarding.css `.ts` transform). */
const POSE_MS = 1300
/** What's on the screen changes this long before its move ends (the move's last stretch hardly moves). */
const POSE_OVERLAP_MS = 250

/** The dive into the screen, before the companion. */
const DIVE_MS = 1100

/**
 * The companion's first run. The truck's screen stands in a dark room, the live WestPlay on it (the cabin
 * page in its demo mode): it boots, steps aside while the player sets WestPlay up, and tells them how it's
 * going through banners as CarPlay would; it opens what each step is about (their map as it's made, an app
 * tapped with the key they choose), and comes back to centre stage on the Dashboard, the player diving into
 * it as they finish. The game stays closed until the end: WestPlay installs into it and builds its map
 * while it's closed.
 */
export function Onboarding({
  initial,
  status,
  settings,
  onFinished,
  mapAddress = defaultMapAddress,
}: {
  initial: SetupState
  status: HostStatus | null
  settings: SettingsState
  /** The onboarding's done: its tutorial next, from its start or skipped to its end. */
  onFinished(start: 'lesson' | 'launch'): void
  mapAddress?: (status: HostStatus | null) => { base: string; built: number | null } | null
}) {
  const [step, setStep] = useState<StepId>('welcome')
  const [leaving, setLeaving] = useState(false)
  const [finishing, setFinishing] = useState(false)
  const [game, setGame] = useState<GameInfo | null>(initial.game ?? initial.found[0] ?? null)
  const [check, setCheck] = useState<GameCheck | null>(null)
  // What installing did, kept while the player moves between steps (until they choose another game).
  const [installed, setInstalled] = useState<InstallReport | null>(null)
  const demo = useDemo()
  const [powered, setPowered] = useState(false)
  const [banner, setBanner] = useState<Banner | null>(null)
  const [scanning, setScanning] = useState(false)
  const [glow, setGlow] = useState(GLOW.boot)
  const [cursor, setCursor] = useState<{ x: number; y: number; pressing: boolean } | null>(null)
  const root = useRef<HTMLDivElement>(null)
  const index = STEPS.indexOf(step)
  // The game's language, as WestPlay speaks in the cab (and the map's place names are in); Windows' for a
  // game never started.
  const [code] = useState(() => initial.language ?? languageOf(navigator.languages))
  const words = wordsFor(code)

  // When the screen's move to its next place ends. Its content changes after that, not during it: the
  // page draws both, and an app or the Dashboard starting up (the map) holds up the move, which then jumps.
  const moveEnds = useRef(0)
  const go = (to: number) => {
    if (leaving || to < 0 || to >= STEPS.length) return
    if (POSES[STEPS[to]] !== POSES[step]) moveEnds.current = performance.now() + LEAVE_MS + POSE_MS
    setLeaving(true)
    window.setTimeout(() => {
      setStep(STEPS[to])
      setLeaving(false)
    }, LEAVE_MS)
  }
  const next = () => go(index + 1)
  const back = index > 1 ? () => go(index - 1) : undefined

  const { send } = demo
  // The last place the screen was sent, and when: the same again right after is dropped. React runs a
  // step's effects twice while developing, and the screen takes a second press during an animation as
  // a player's, finishing it at once: an app would appear without opening.
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
    settling: () => Math.max(0, moveEnds.current - POSE_OVERLAP_MS - performance.now()),
  }

  // The screen comes on once WestPlay in it is ready: the intro, then Home.
  useEffect(() => {
    if (!demo.ready || powered) return
    const timer = window.setTimeout(() => {
      send({ type: 'power', on: true })
      setPowered(true)
    }, 700)
    return () => window.clearTimeout(timer)
  }, [demo.ready, powered, send])

  // WestPlay on the screen speaks the same language.
  useEffect(() => {
    if (demo.ready) send({ type: 'language', code })
  }, [demo.ready, code, send])

  // The map, once there is one, is shown on the screen.
  const map = mapAddress(status)
  const mapKey = map ? `${map.base}#${map.built}` : ''
  useEffect(() => {
    if (map) send({ type: 'map', base: map.base, built: map.built })
    // Keyed by the map's address and build.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mapKey, demo.ready, send])

  // The mouse turns the screen a little.
  const tilt = (event: React.PointerEvent) => {
    const box = root.current?.getBoundingClientRect()
    if (!box) return
    root.current?.style.setProperty('--tilt-x', (((event.clientX - box.left) / box.width) * 2 - 1).toFixed(3))
    root.current?.style.setProperty('--tilt-y', (((event.clientY - box.top) / box.height) * 2 - 1).toFixed(3))
  }

  const finish = (start: 'lesson' | 'launch') => {
    setFinishing(true)
    window.setTimeout(() => onFinished(start), DIVE_MS)
  }

  return (
    <div
      ref={root}
      className="ob"
      lang={words.locale}
      data-power={powered || undefined}
      data-finishing={finishing || undefined}
      onPointerMove={tilt}
    >
      <TruckScreen
        frame={demo.frame}
        ready={demo.ready}
        pose={finishing ? 'dive' : POSES[step]}
        glow={glow}
        banner={banner}
        scanning={scanning}
      >
        <GlassCursor
          x={cursor?.x ?? 640}
          y={cursor?.y ?? 420}
          pressing={cursor?.pressing ?? false}
          visible={cursor !== null}
        />
      </TruckScreen>
      <WordsContext.Provider value={{ words, code }}>
        <RoomText.Provider value={{ locale: words.locale, back: words.back }}>
          <ScreenContext.Provider value={screen}>
            {step === 'welcome' && <Welcome next={next} />}
            {step === 'game' && (
              <ChooseGame
                found={initial.found}
                game={game}
                choose={(chosen) => {
                  setGame(chosen)
                  setCheck(null)
                  setInstalled(null)
                }}
                next={next}
                back={back}
              />
            )}
            {step === 'check' && game && (
              <CheckGame game={game} check={check} checked={setCheck} next={next} back={back} />
            )}
            {step === 'install' && game && (
              <Install game={game} report={installed} reported={setInstalled} next={next} back={back} />
            )}
            {step === 'map' && <MapStep status={status} next={next} back={back} />}
            {step === 'controls' && <Controls settings={settings} next={next} back={back} />}
            {step === 'accounts' && <Accounts status={status} next={next} back={back} />}
            {step === 'spotify' && <ServiceSignIn service="spotify" status={status} next={next} back={back} />}
            {step === 'appleMusic' && <ServiceSignIn service="appleMusic" status={status} next={next} back={back} />}
            {step === 'tutorial' && <TutorialIntro code={code} finish={finish} back={back} />}
          </ScreenContext.Provider>
        </RoomText.Provider>
      </WordsContext.Provider>
      <div className="ob-dots" aria-hidden>
        {STEPS.map((id, at) => (
          <i key={id} data-here={at === index || undefined} data-passed={at < index || undefined} />
        ))}
      </div>
    </div>
  )
}

function Welcome({ next }: { next(): void }) {
  const screen = useScreen()
  // The words come once WestPlay has booted on the screen.
  const [booted, setBooted] = useState(false)
  useEffect(() => {
    if (!screen.powered) return
    const timer = window.setTimeout(() => setBooted(true), BOOT_MS)
    return () => window.clearTimeout(timer)
  }, [screen.powered])
  const words = useWords()
  useScreenAt('home', GLOW.home, null)
  useTour(
    booted
      ? [
          { tap: ON_SCREEN.page(2), then: 3400 },
          { tap: ON_SCREEN.page(1), then: 4200 },
        ]
      : null,
    2200,
  )
  if (!booted) return null
  return (
    <Panel
      place="below"
      title={words.welcome.title}
      sub={words.welcome.sub}
      action={<Button onClick={next}>{words.welcome.start}</Button>}
    />
  )
}

function ChooseGame({
  found,
  game,
  choose,
  next,
  back,
}: {
  found: GameInfo[]
  game: GameInfo | null
  choose(game: GameInfo): void
  next(): void
  back?: () => void
}) {
  const [games, setGames] = useState(() =>
    game && !found.some((known) => known.dir === game.dir) ? [...found, game] : found,
  )
  const [error, setError] = useState<string | null>(null)
  const running = useGameRunning()
  const words = useWords()
  const w = words.game
  useScreenAt(
    'dashboard',
    GLOW.dashboard,
    game
      ? { title: 'Euro Truck Simulator 2', detail: gameSummary(words, game) }
      : { title: w.titleMissing, detail: w.bannerMissing },
  )

  const pick = async () => {
    try {
      const chosen = await chooseGameFolder(w.picker)
      if (!chosen) return
      setError(null)
      if (!games.some((known) => known.dir === chosen.dir)) setGames([...games, chosen])
      choose(chosen)
    } catch (e) {
      setError(explain(words, e))
    }
  }

  return (
    <Panel
      place="side"
      title={games.length ? w.title : w.titleMissing}
      sub={games.length ? (games.length > 1 ? w.foundMany : w.found) : w.missing}
      back={back}
      action={
        <Button onClick={next} disabled={!game || running !== false}>
          {words.continue}
        </Button>
      }
    >
      <Group place={4}>
        {games.map((known) => (
          <Row
            key={known.dir}
            title={known.dir.replace(/[\\/]steamapps[\\/]common[\\/][^\\/]+[\\/]?$/i, '')}
            detail={gameSummary(words, known)}
            accessory={known.dir === game?.dir ? <Tick key={known.dir} /> : undefined}
            onClick={() => choose(known)}
          />
        ))}
        <Row title={games.length ? w.chooseAnother : w.chooseFolder} link onClick={pick} />
      </Group>
      {error && <Note tone="bad">{error}</Note>}
      {running && (
        <Note>
          <b>{w.closeToGoOn.title}</b> {w.closeToGoOn.text}
        </Note>
      )}
    </Panel>
  )
}

function CheckGame({
  game,
  check,
  checked,
  next,
  back,
}: {
  game: GameInfo
  check: GameCheck | null
  checked(check: GameCheck): void
  next(): void
  back?: () => void
}) {
  const screen = useScreen()
  const words = useWords()
  const w = words.check
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    if (check) return
    let active = true
    // Long enough to see the scan pass at least once.
    const shown = new Promise((resolve) => window.setTimeout(resolve, 2200))
    Promise.all([checkGame(game.dir), shown])
      .then(([result]) => active && checked(result))
      .catch((e: unknown) => active && setError(explain(words, e)))
    return () => {
      active = false
    }
  }, [game.dir, check, checked, words])
  useEffect(() => {
    screen.scan(!check && !error)
    return () => screen.scan(false)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [check, error])
  useScreenAt(
    'home',
    check ? GLOW.home : GLOW.scan,
    check
      ? {
          title: check.modProblem ? w.notYet : w.checked,
          detail: `${count(words, w.profileCount, check.profiles.length)} · ${count(words, words.summary.dlcs, check.dlcs)}`,
        }
      : { title: w.checking, busy: true },
  )

  if (!check) {
    return (
      <Panel
        place="side"
        title={w.looking}
        sub={w.lookingSub}
        back={back}
        action={
          <Button onClick={next} busy={!error}>
            {words.continue}
          </Button>
        }
      >
        {error && <Note tone="bad">{error}</Note>}
      </Panel>
    )
  }

  return (
    <Panel
      place="side"
      title={check.modProblem ? w.titleNotYet : w.title}
      back={back}
      action={
        <Button onClick={next} disabled={!!check.modProblem}>
          {words.continue}
        </Button>
      }
    >
      {check.modProblem && (
        <Note tone="bad">
          <b>{w.cantInstall}</b> {check.modProblem}
        </Note>
      )}
      {check.otherApps.includes('ymnOne') && (
        <Note tone="bad">
          <b>{w.otherCarPlay.title}</b> {w.otherCarPlay.text}
        </Note>
      )}
      <Group place={2}>
        <Row
          title={w.version}
          value={check.game.version ?? w.unknown}
          accessory={check.game.tested ? <Tick done /> : <Status tone="idle" />}
        />
        <Row title={w.dlcs} value={new Intl.NumberFormat(words.locale).format(check.dlcs)} />
        <Row
          title={w.mapDlcs}
          detail={check.mapDlcs.length ? list(words, check.mapDlcs) : w.noMapDlcs}
          value={new Intl.NumberFormat(words.locale).format(check.mapDlcs.length)}
        />
      </Group>
      {!check.game.tested && !check.modProblem && <Note>{w.untested}</Note>}
      <Group title={w.trucks} place={3}>
        {check.trucks.map((truck) => (
          <Row
            key={truck.id}
            title={
              truck.id === 'scania-7'
                ? w.scania7
                : truck.id === 'scania-smart-dash'
                  ? w.scaniaSmartDash
                  : truck.id === 'any-truck-tablet'
                    ? w.anyTruckTablet
                    : truck.name
            }
            detail={
              truck.fit === 'ready'
                ? undefined
                : truck.fit === 'notYet'
                  ? w.truckNotYet
                  : truck.fromMod
                    ? w.notInMods
                    : w.notInGame
            }
            accessory={truck.fit === 'ready' ? <Tick done /> : <Cross />}
          />
        ))}
      </Group>
      <p className="ob-sub ob-in" style={{ ...n(3), fontSize: 12.5, marginTop: 0 }}>
        {w.otherTrucks}
      </p>
      <Group title={w.profiles} place={4}>
        {check.profiles.length ? (
          check.profiles.map((profile) => <ProfileRow key={profile.dir} profile={profile} />)
        ) : (
          <Row title={w.noProfiles} detail={w.noProfilesDetail} />
        )}
      </Group>
      <p className="ob-sub ob-in" style={{ ...n(5), fontSize: 12.5, marginTop: 0 }}>
        {w.mapNote}
      </p>
    </Panel>
  )
}

function Install({
  game,
  report,
  reported,
  next,
  back,
}: {
  game: GameInfo
  /** What installing did, if it was done (on this visit or an earlier one). */
  report: InstallReport | null
  reported(report: InstallReport): void
  next(): void
  back?: () => void
}) {
  const screen = useScreen()
  const words = useWords()
  const w = words.install
  const [installing, setInstalling] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // Back on a finished install, it's simply done: no reboot again.
  const [celebrated, setCelebrated] = useState(report?.done ?? false)
  const running = useGameRunning()

  const install = async (administrator: boolean) => {
    setInstalling(true)
    setError(null)
    try {
      const result = await installIntoGame(game.dir, administrator)
      reported(result)
      if (result.done) {
        // WestPlay in the game: the screen boots it again.
        screen.reboot()
        window.setTimeout(() => setCelebrated(true), BOOT_MS + 450)
      }
    } catch (e) {
      setError(explain(words, e))
    } finally {
      setInstalling(false)
    }
  }

  const plugin = report?.plugin
  const done = report?.done ?? false
  useScreenAt(
    'home',
    done ? GLOW.boot : GLOW.home,
    installing
      ? banner({ ...w.banners.installing, text: w.plugin }, true)
      : done
        ? celebrated
          ? banner(w.banners.done)
          : null
        : plugin?.state === 'needsAdministrator' || plugin?.state === 'declined'
          ? banner(w.banners.permission)
          : plugin?.state === 'inUse'
            ? banner(w.banners.inUse)
            : report && !done
              ? banner(w.banners.failed)
              : banner({ ...w.banners.ready, text: w.pluginDetail }),
  )

  // Waiting on the player (Windows' permission, the game closed) isn't a failure.
  const pluginTone = installing
    ? 'busy'
    : plugin?.state === 'installed'
      ? 'good'
      : plugin?.state === 'failed'
        ? 'bad'
        : 'idle'
  const asAdministrator = plugin?.state === 'needsAdministrator' || plugin?.state === 'declined'

  return (
    <Panel
      place="side"
      title={done ? w.titleDone : w.title}
      sub={w.sub}
      // While WestPlay boots again on the screen, the player waits for it: the next step opens an app there.
      back={installing || (done && !celebrated) ? undefined : back}
      action={
        done ? (
          <Button onClick={next} busy={!celebrated}>
            {words.continue}
          </Button>
        ) : (
          <Button onClick={() => install(asAdministrator)} busy={installing} disabled={running !== false}>
            {asAdministrator ? w.asAdministrator : report || error ? w.tryAgain : w.install}
          </Button>
        )
      }
    >
      <Group place={4}>
        <Row
          title={w.plugin}
          detail={w.pluginDetail}
          accessory={pluginTone === 'good' ? <Tick done /> : <Status tone={pluginTone} />}
        />
      </Group>
      {running && !done && (
        <Note>
          <b>{w.closeToInstall.title}</b> {w.closeToInstall.text}
        </Note>
      )}
      {plugin?.state === 'inUse' && (
        <Note>
          <b>{w.inUse.title}</b> {w.inUse.text}
        </Note>
      )}
      {plugin?.state === 'needsAdministrator' && (
        <Note tone="calm">
          <b>{w.needsAdministrator.title}</b> {w.needsAdministrator.text}
        </Note>
      )}
      {plugin?.state === 'declined' && (
        <Note>
          <b>{w.declined.title}</b> {w.declined.text}
        </Note>
      )}
      {plugin?.state === 'failed' && (
        <Note tone="bad">
          <b>{w.pluginFailed}</b> {explain(words, plugin.error)}
        </Note>
      )}
      {report?.modError && (
        <Note tone="bad">
          <b>{w.modFailed}</b> {explain(words, report.modError)}
        </Note>
      )}
      {error && <Note tone="bad">{error}</Note>}
    </Panel>
  )
}

function MapStep({ status, next, back }: { status: HostStatus | null; next(): void; back?: () => void }) {
  const map = status?.map ?? { state: 'unknown' as const }
  const building = map.state === 'building' ? map : null
  const shown = useGlidingProgress(building?.done ?? null, building?.stage ?? null)
  // Seen building on this visit: once it's ready, its stages stay up, all ticked.
  const [watched, setWatched] = useState(false)
  if (building && !watched) setWatched(true)
  const settled = map.state === 'ready' || map.state === 'notAllowed' || map.state === 'failed'
  const { words, code } = useContext(WordsContext)
  const w = words.map
  // The HVT message, its two sentences as the banner's title and line.
  const [hvtTitle, hvtText] = [
    ...new Intl.Segmenter(words.locale, { granularity: 'sentence' }).segment(hvtMapMessage(code)),
  ].map((part) => part.segment.trim())
  useScreenAt(
    'maps',
    GLOW.maps,
    map.state === 'building'
      ? {
          title: w.banners.building,
          detail: `${w.stages[map.stage].now} · ${percent(words, shown)}`,
          progress: shown,
        }
      : map.state === 'ready'
        ? banner(w.banners.ready)
        : map.state === 'waitingForGame'
          ? banner(w.banners.waiting)
          : map.state === 'waitingForMemory'
            ? banner(companionWords(code).map.memory.banner)
            : map.state === 'notAllowed'
              ? { title: hvtTitle, detail: hvtText }
              : map.state === 'failed'
                ? banner(w.banners.failed)
                : { title: w.banners.getting, busy: true },
  )

  useTour(
    map.state === 'ready'
      ? [
          { tap: ON_SCREEN.zoomOut, then: 700 },
          { tap: ON_SCREEN.zoomOut, then: 700 },
          { tap: ON_SCREEN.zoomOut, then: 3200 },
          { tap: ON_SCREEN.zoomIn, then: 700 },
          { tap: ON_SCREEN.zoomIn, then: 700 },
          { tap: ON_SCREEN.zoomIn, then: 3600 },
        ]
      : null,
    1800,
  )

  return (
    <Panel
      place="side"
      title={map.state === 'ready' ? w.titleReady : w.title}
      sub={w.sub}
      back={back}
      action={
        <Button onClick={next} disabled={!settled}>
          {words.continue}
        </Button>
      }
    >
      {(building || (watched && map.state === 'ready')) && (
        <MapProgress shown={shown} stage={building?.stage ?? null} finished={map.state === 'ready'} />
      )}
      {map.state === 'failed' && <Note tone="bad">{map.error}</Note>}
      {map.state === 'waitingForGame' && (
        <Note>
          <b>{w.waiting.title}</b> {w.waiting.text}
        </Note>
      )}
      {map.state === 'waitingForMemory' && (
        <Note>
          <b>{companionWords(code).map.memory.note.title}</b>{' '}
          {memoryNote(companionWords(code), words.locale, map.needed, map.free)}
        </Note>
      )}
    </Panel>
  )
}

function Controls({ settings, next, back }: { settings: SettingsState; next(): void; back?: () => void }) {
  const current = settings.settings
  const words = useWords()
  useScreenAt('home', GLOW.home, null)
  // Holding the key: the player taps one app, then another.
  useTour([
    ...openAndClose('music', GLOW.music),
    ...openAndClose('weather', GLOW.weather),
    ...openAndClose('maps', GLOW.maps),
  ])
  return (
    <Panel
      place="side"
      title={words.controls.title}
      sub={words.controls.sub}
      back={back}
      action={<Button onClick={next}>{words.continue}</Button>}
    >
      <KeyChooser
        value={current?.interaction.key ?? DEFAULT_KEY}
        disabled={!current}
        place={4}
        choose={(key) => current && settings.update({ ...current, interaction: { ...current.interaction, key } })}
      />
      {settings.error && <Note tone="bad">{settings.error}</Note>}
    </Panel>
  )
}

function Accounts({ status, next, back }: { status: HostStatus | null; next(): void; back?: () => void }) {
  const signedIn = status?.youtubeSignedIn ?? false
  const signingIn = status?.youtubeSigningIn ?? false
  // Signed in already? Found out first (YouTube run, hidden, to say so).
  const checking = useChecking(status?.youtubeSignedIn != null, youtubeCheck)
  const signOut = useSignOut(signedIn, youtubeSignOut)
  const words = useWords()
  const w = words.accounts
  useScreenAt('youtube', GLOW.youtube, signingIn ? banner(w.signingIn, true) : null)
  return (
    <Panel
      place="side"
      title={w.title}
      sub={w.sub}
      back={back}
      action={
        <Button onClick={next} disabled={checking}>
          {signedIn ? words.continue : w.notNow}
        </Button>
      }
    >
      <Group place={4}>
        <Row
          title="YouTube"
          detail={checking ? undefined : signingIn ? w.finishInWindow : signedIn ? w.signedIn : w.notSignedIn}
          accessory={
            checking ? (
              <Checking />
            ) : signedIn ? (
              <Button glass small busy={signOut.signingOut} onClick={signOut.start}>
                {w.signOut}
              </Button>
            ) : (
              <Button glass small busy={signingIn} onClick={() => youtubeSignIn()}>
                {w.signIn}
              </Button>
            )
          }
        />
      </Group>
    </Panel>
  )
}

/** Spotify's and Apple Music's sign-ins, as YouTube's: the service's own page in a window on the desktop, its
 * app on the screen meanwhile. Their buttons and states are YouTube's step's words; their own say what each
 * is for. */
const SERVICE_STEPS = {
  spotify: {
    name: 'Spotify',
    app: 'spotify',
    glow: GLOW.spotify,
    signedIn: (status: HostStatus | null) => status?.spotifySignedIn,
    signingIn: (status: HostStatus | null) => status?.spotifySigningIn ?? false,
    check: spotifyCheck,
    signIn: spotifySignIn,
    signOut: spotifySignOut,
  },
  appleMusic: {
    name: 'Apple Music',
    app: 'music',
    glow: GLOW.music,
    signedIn: (status: HostStatus | null) => status?.appleMusicSignedIn,
    signingIn: (status: HostStatus | null) => status?.appleMusicSigningIn ?? false,
    check: appleMusicCheck,
    signIn: appleMusicSignIn,
    signOut: appleMusicSignOut,
  },
} as const

function ServiceSignIn({
  service,
  status,
  next,
  back,
}: {
  service: keyof typeof SERVICE_STEPS
  status: HostStatus | null
  next(): void
  back?: () => void
}) {
  const it = SERVICE_STEPS[service]
  const signedIn = it.signedIn(status) ?? false
  const signingIn = it.signingIn(status)
  // Signed in already? Found out first (the service's web player run, hidden, to say so).
  const checking = useChecking(it.signedIn(status) != null, it.check)
  const signOut = useSignOut(signedIn, it.signOut)
  const words = useWords()
  const w = words.accounts
  const s = words[service]
  useScreenAt(it.app, it.glow, signingIn ? banner(s.signingIn, true) : null)
  return (
    <Panel
      place="side"
      title={s.title}
      sub={s.sub}
      back={back}
      action={
        <Button onClick={next} disabled={checking}>
          {signedIn ? words.continue : w.notNow}
        </Button>
      }
    >
      <Group place={4}>
        <Row
          title={it.name}
          detail={checking ? undefined : signingIn ? s.finishInWindow : signedIn ? w.signedIn : w.notSignedIn}
          accessory={
            checking ? (
              <Checking />
            ) : signedIn ? (
              <Button glass small busy={signOut.signingOut} onClick={signOut.start}>
                {w.signOut}
              </Button>
            ) : (
              <Button glass small busy={signingIn} onClick={() => it.signIn()}>
                {w.signIn}
              </Button>
            )
          }
        />
      </Group>
    </Panel>
  )
}

/** The onboarding's last step: the tutorial's start (the player dives into the screen and learns it), or
 * skipped to its end (what to do in the game). */
function TutorialIntro({
  code,
  finish,
  back,
}: {
  code: string
  finish(start: 'lesson' | 'launch'): void
  back?: () => void
}) {
  const w = companionWords(code).tutorial.intro
  useScreenAt('dashboard', GLOW.dashboard, null)
  useTour(
    [
      { tap: ON_SCREEN.homeButton, glow: GLOW.home, then: 3400 },
      { tap: ON_SCREEN.homeButton, glow: GLOW.dashboard, then: 4600 },
    ],
    2600,
  )
  return (
    <Panel
      place="below"
      title={w.title}
      sub={w.sub}
      back={back}
      action={
        <>
          <Button glass onClick={() => finish('launch')}>
            {w.skip}
          </Button>
          <Button onClick={() => finish('lesson')}>{w.start}</Button>
        </>
      }
    />
  )
}
