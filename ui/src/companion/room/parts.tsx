import { type CSSProperties, type PointerEvent, type ReactNode, useContext } from 'react'
import { hvtMapMessage } from '../../cabin/maps/blockedMap'
import { count, fill, list, percent, type Words } from '../onboarding/words'
import type { ProfileCheck } from '../setup'
import type { MapStage, MapStatus } from '../useHostStatus'
import { KEY_PRESETS, type KeyCode, type KeyPreset } from '../useSettings'
import { headline, MAP_STAGES, n, RoomText, useWords, WordsContext } from './room'
import { useScreen } from './screen'

/** iOS's back chevron. */
function BackGlyph() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M14.5 5.5L8 12l6.5 6.5" />
    </svg>
  )
}

/* The room's parts: a page's words (its headline rising letter by letter), iOS's grouped lists, capsule
   buttons, ticks that draw themselves, notes. Shared by the onboarding and the companion (moved from the
   onboarding, as they were). */

/** A step's words: its headline, rising letter by letter; a line under it; what it shows; its buttons. */
export function Panel({
  place,
  title,
  sub,
  children,
  back,
  action,
}: {
  /** Beside the screen, under it, or a companion page (between its list and the screen). */
  place: 'side' | 'below' | 'page'
  title: string
  sub?: ReactNode
  children?: ReactNode
  back?: () => void
  action?: ReactNode
}) {
  const screen = useScreen()
  const text = useContext(RoomText)
  return (
    <section className="ob-words" data-place={place} data-leaving={screen.leaving || undefined}>
      {/* A new headline rises in again. */}
      <h1 key={title} className="ob-letters" aria-label={title}>
        {headline(title, text.locale).map((word, w) => (
          <span key={w} className="ob-word" aria-hidden>
            {word.letters.map((letter, l) => (
              <span key={l} style={n(word.start + l)}>
                {letter}
              </span>
            ))}
          </span>
        ))}
      </h1>
      {sub && (
        <p className="ob-sub ob-in" style={n(3)}>
          {sub}
        </p>
      )}
      {children && <div className="ob-content">{children}</div>}
      {(back || action) && (
        <div className="ob-actions ob-in" style={n(6)}>
          {back && (
            <button type="button" className="ob-back" onClick={back} aria-label={text.back}>
              <BackGlyph />
            </button>
          )}
          {action}
        </div>
      )}
    </section>
  )
}

export function Button({
  children,
  onClick,
  disabled,
  busy,
  glass,
  small,
  danger,
}: {
  children: ReactNode
  onClick(): void
  disabled?: boolean
  busy?: boolean
  glass?: boolean
  small?: boolean
  /** It can't be undone (uninstalling): iOS's red. */
  danger?: boolean
}) {
  return (
    <button
      type="button"
      className="ob-button"
      data-glass={glass || undefined}
      data-small={small || undefined}
      data-danger={danger || undefined}
      onClick={onClick}
      disabled={disabled || busy}
    >
      {busy && <span className="ob-spinner" />}
      {children}
    </button>
  )
}

export function Group({ title, children, place }: { title?: string; children: ReactNode; place: number }) {
  return (
    <>
      {title && (
        <p className="ob-group-title ob-in" style={n(place)}>
          {title}
        </p>
      )}
      <div className="ob-group ob-in" style={n(place)}>
        {children}
      </div>
    </>
  )
}

export function Row({
  title,
  detail,
  value,
  accessory,
  onClick,
  link,
  danger,
  hover,
}: {
  title: string
  detail?: string
  value?: string
  accessory?: ReactNode
  onClick?: () => void
  link?: boolean
  /** It can't be undone: its words in iOS's red. */
  danger?: boolean
  /** The pointer came over it. */
  hover?: () => void
}) {
  const content = (
    <>
      <span className="ob-row-text">
        <b title={title}>{title}</b>
        {detail && <span>{detail}</span>}
      </span>
      {value && <span className="ob-row-value">{value}</span>}
      {accessory}
    </>
  )
  return onClick ? (
    <button
      type="button"
      className="ob-row"
      data-link={link || undefined}
      data-danger={danger || undefined}
      onClick={onClick}
      onPointerEnter={hover}
    >
      {content}
    </button>
  ) : (
    <div className="ob-row" onPointerEnter={hover}>
      {content}
    </div>
  )
}

export function Tick({ done }: { done?: boolean }) {
  return (
    <svg
      className="ob-tick"
      data-done={done || undefined}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.6"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M5 12.5l4.5 4.5L19 7.5" />
    </svg>
  )
}

/** A cross that draws itself: what won't work for the player. */
export function Cross() {
  return (
    <svg
      className="ob-tick"
      data-cross
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.6"
      strokeLinecap="round"
    >
      <path d="M7 7l10 10" />
      <path d="M17 7L7 17" />
    </svg>
  )
}

export function Status({ tone }: { tone: 'good' | 'bad' | 'busy' | 'idle' }) {
  return <i className="ob-status" data-tone={tone} />
}

export function Note({ tone, children }: { tone?: 'bad' | 'calm'; children: ReactNode }) {
  return (
    <p className="ob-note ob-in" data-tone={tone} style={n(5)}>
      <span>{children}</span>
    </p>
  )
}

/** A sign-in row's spinner while it's found out whether the service is signed in. */
export function Checking() {
  return <span className="ob-spinner ob-row-spinner" aria-hidden />
}

/** The build as it goes: a large percentage, a bar, and what it's doing, each stage joining the list as it
 * starts and ticked off as it ends. */
export function MapProgress({ shown, stage, finished }: { shown: number; stage: MapStage | null; finished: boolean }) {
  const words = useWords()
  const reached = finished ? MAP_STAGES.length - 1 : MAP_STAGES.findIndex((known) => known.stage === stage)
  return (
    <div className="ob-build ob-in" style={n(4)}>
      <span className="ob-percent">{percent(words, finished ? 1 : shown)}</span>
      <i className="ob-bar">
        <i style={{ width: `${(finished ? 1 : shown) * 100}%` }} />
      </i>
      <ol className="ob-stages">
        {MAP_STAGES.slice(0, reached + 1).map(({ stage: each }, at) => {
          const over = finished || at < reached
          return (
            <li key={each} data-now={!over || undefined}>
              {over ? <Tick done /> : <i className="ob-working" />}
              <span>{over ? words.map.stages[each].done : words.map.stages[each].now}</span>
            </li>
          )
        })}
      </ol>
    </div>
  )
}

const KEYS: { value: KeyPreset; cap: (words: Words) => ReactNode }[] = [
  { value: 'leftAlt', cap: (words) => <span className="ob-cap">{words.controls.alt}</span> },
  { value: 'rightCtrl', cap: (words) => <span className="ob-cap">{words.controls.ctrl}</span> },
  { value: 'mouseBack', cap: () => <Mouse side="back" /> },
  { value: 'mouseForward', cap: () => <Mouse side="forward" /> },
]

function Mouse({ side }: { side: 'back' | 'forward' }) {
  return (
    <svg className="ob-mouse" viewBox="0 0 26 40" fill="none" stroke="currentColor" strokeWidth="1.6">
      <rect x="3" y="1.5" width="21" height="37" rx="10.5" />
      <path d="M13.5 1.5v11" />
      <rect className="side" x="0.5" y={side === 'back' ? 19 : 11} width="4" height="6" rx="2" stroke="none" />
    </svg>
  )
}

/** A profile and its map: what it plays, and its map's state once the host has said (each profile gets one,
 * made one by one): ticked when ready, a spinner while it waits or is made, a cross and why when it can't be.
 * `leftOut`: the map mods its map was made without, their files unreadable (named, once it's made). */
export function ProfileRow({
  profile,
  map: state,
  leftOut = [],
}: {
  profile: ProfileCheck
  map?: MapStatus
  leftOut?: string[]
}) {
  const { words, code } = useContext(WordsContext)
  const w = words.check
  if (profile.hvtMap || state?.state === 'notAllowed') {
    return <Row title={profile.name} detail={hvtMapMessage(code)} accessory={<Cross />} />
  }
  if (state?.state === 'failed') {
    return <Row title={profile.name} detail={state.error} accessory={<Cross />} />
  }
  const map = profile.map === 'europe' ? w.baseMap : profile.map
  const withMods = profile.mapMods.length ? fill(w.withMods, { map, mods: list(words, profile.mapMods) }) : map
  // Once its map is made, the mods it was made without, by name; before, how many the check couldn't read.
  const unreadable = leftOut.length
    ? ` (${count(words, w.unreadable, leftOut.length)}: ${list(words, leftOut)})`
    : profile.unreadable > 0
      ? ` (${count(words, w.unreadable, profile.unreadable)})`
      : ''
  const building = state?.state === 'building' ? ` · ${percent(words, state.done)}` : ''
  const waiting = state !== undefined && state.state !== 'ready'
  return (
    <Row
      title={profile.name}
      detail={`${withMods}${unreadable}${building}`}
      accessory={waiting ? <Checking /> : <Tick done />}
    />
  )
}

/** The keys a player can hold to use the screen, as keycaps (and the mouse's side buttons): the chosen one
 * pressed down. */
export function KeyChooser({
  value,
  disabled,
  place,
  choose,
}: {
  value: KeyCode
  disabled?: boolean
  place: number
  choose(key: KeyCode): void
}) {
  const words = useWords()
  return (
    <div className="ob-keys ob-in" style={n(place)}>
      {KEYS.map((key) => (
        <button
          type="button"
          key={key.value}
          className="ob-key"
          data-chosen={KEY_PRESETS[key.value] === value || undefined}
          disabled={disabled}
          onClick={() => choose(KEY_PRESETS[key.value])}
        >
          {key.cap(words)}
          {words.controls.keys[key.value]}
        </button>
      ))}
    </div>
  )
}

/** iOS's switch: its knob slides over on a spring, and stretches while it's pressed. */
export function Switch({
  on,
  disabled,
  label,
  change,
}: {
  on: boolean
  disabled?: boolean
  label: string
  change(on: boolean): void
}) {
  return (
    <button
      type="button"
      role="switch"
      className="ob-switch"
      aria-checked={on}
      aria-label={label}
      data-on={on || undefined}
      disabled={disabled}
      onClick={() => change(!on)}
    >
      <i />
    </button>
  )
}

/** iOS's slider: dragged (or clicked along), the value follows; let go, `commit` has it. */
export function Slider({
  value,
  min,
  max,
  step,
  label,
  change,
  commit,
}: {
  value: number
  min: number
  max: number
  step: number
  label: string
  change(value: number): void
  commit(value: number): void
}) {
  const at = (event: PointerEvent<HTMLDivElement>) => {
    const box = event.currentTarget.getBoundingClientRect()
    const share = Math.min(1, Math.max(0, (event.clientX - box.left) / box.width))
    return Math.round((min + share * (max - min)) / step) * step
  }
  const share = (value - min) / (max - min)
  return (
    <div
      className="ob-slider"
      role="slider"
      aria-label={label}
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={value}
      style={{ '--share': share } as CSSProperties}
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId)
        event.currentTarget.dataset.dragging = ''
        change(at(event))
      }}
      onPointerMove={(event) => {
        if (event.currentTarget.hasPointerCapture(event.pointerId)) change(at(event))
      }}
      onPointerUp={(event) => {
        delete event.currentTarget.dataset.dragging
        commit(at(event))
      }}
      onPointerCancel={(event) => delete event.currentTarget.dataset.dragging}
    >
      <i className="ob-slider-fill" />
      <i className="ob-slider-thumb" />
    </div>
  )
}
