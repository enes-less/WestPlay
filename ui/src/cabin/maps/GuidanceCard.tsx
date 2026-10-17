import { useEffect, useMemo, useState } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { formatTripDistance } from '../../shared/format'
import { useLive } from '../state/store'
import { type Strings, useText } from '../state/text'
import { guidanceOf, type Maneuver, NOW_WITHIN, type Step } from './guidance'
import { ManeuverGlyph } from './ManeuverGlyph'

/** A step as the store can compare it: the maneuver itself (the route's own object, the same from one
 * read to the next) or `'arrive'`. */
type StepRef = Maneuver | 'arrive'

function refOf(step: Step | null): StepRef | null {
  return step && (step.kind === 'arrive' ? 'arrive' : step.maneuver)
}

function stepOf(ref: StepRef): Step {
  return ref === 'arrive' ? { kind: 'arrive' } : { kind: 'turn', maneuver: ref }
}

/** The route's next step as shown: what, how far (already worded, so the card redraws only when the words
 * change), and what follows right after. `step` is `null` without a route. Read as values that stay the
 * same while nothing changes (a new object on every read would redraw the card forever). */
function useGuidance(): { step: Step | null; distance: string; then: Step | null } {
  const text = useText()
  const { step, distance, then } = useLive(
    useShallow((state) => {
      const { route, routeNext, telemetry } = state
      const guidance = route
        ? guidanceOf(route.distances, route.maneuvers, routeNext, telemetry?.navigationDistance ?? 0)
        : null
      if (!guidance) return { step: null, distance: '', then: null }
      const distance = guidance.distance < NOW_WITHIN ? text.now : formatTripDistance(guidance.distance, text.decimal)
      return { step: refOf(guidance.step), distance, then: refOf(guidance.then) }
    }),
  )
  return useMemo(() => ({ step: step && stepOf(step), distance, then: then && stepOf(then) }), [step, distance, then])
}

/** What a step says: "Turn right", "At the roundabout, take the second exit". */
function instructionFor(step: Step, text: Strings): string {
  if (step.kind === 'arrive') return text.arrive
  const { turn, exit } = step.maneuver
  return turn === 'roundabout' ? text.roundabout(exit ?? 1) : text.turns[turn]
}

/** How long a change of step, the card's coming and the "then" row's opening take, ms (the spring the UI
 * opens apps with: `--motion-spring-duration`, shell/transitions.css), and its going. */
const STEP_CHANGE_MS = 560
const CLOSE_MS = 320

/** A step as shown: what, how far, and the step after it. */
interface Shown {
  key: string
  step: Step
  distance: string
  then: Step | null
}

function keyOf(step: Step): string {
  return step.kind === 'arrive' ? 'arrive' : `${step.maneuver.node}:${step.maneuver.turn}`
}

/**
 * The steps on show as the route goes on: the current one (its distance live), the one it replaced while
 * that slides away, and whether the card is closing (the route ended: the last step stays while it goes).
 */
function useStepChange(current: Shown | null): { shown: Shown | null; leaving: Shown | null; closing: boolean } {
  const [shown, setShown] = useState(current)
  const [leaving, setLeaving] = useState<Shown | null>(null)
  // Adjusted while rendering (React's way for state that follows a prop): no frame shows the old state.
  if (current && current !== shown) {
    if (shown && shown.key !== current.key) setLeaving(shown)
    setShown(current)
  }
  const closing = current === null && shown !== null
  useEffect(() => {
    if (!leaving) return
    const timer = setTimeout(() => setLeaving(null), STEP_CHANGE_MS)
    return () => clearTimeout(timer)
  }, [leaving])
  useEffect(() => {
    if (!closing) return
    const timer = setTimeout(() => {
      setShown(null)
      setLeaving(null)
    }, CLOSE_MS)
    return () => clearTimeout(timer)
  }, [closing])
  return { shown, leaving, closing }
}

/** `value`, or while it's `null` the last value it had (to show a row while it closes). */
function useKept<T>(value: T | null): T | null {
  const [kept, setKept] = useState(value)
  if (value !== null && value !== kept) setKept(value)
  return value ?? kept
}

/**
 * Turn-by-turn, CarPlay's way: the next step's arrow, how far it is and what to do, and the step right
 * after it when it's close behind. On the Dashboard it's the top widget; in the Maps app, a banner over
 * the map. Nothing without a route.
 *
 * In motion with Apple's curves: the card springs in when a route starts and fades away when it ends; a
 * step done slides up and away as the next rises in its place; the "then" row opens and closes; and on
 * the Dashboard the widgets below make room for the card, and take it back, as it comes and goes.
 */
export function GuidanceCard({ variant }: { variant: 'widget' | 'banner' }) {
  const text = useText()
  const { step, distance, then } = useGuidance()
  const current = useMemo(() => (step ? { key: keyOf(step), step, distance, then } : null), [step, distance, then])
  const { shown, leaving, closing } = useStepChange(current)
  const thenShown = useKept(shown?.then ?? null)
  const glyphSize = variant === 'widget' ? 62 : 70

  const card = shown && (
    <article
      className={`guidance guidance-${variant} glass${variant === 'widget' ? ' widget' : ''}${closing ? ' closing' : ''}`}
      aria-label={`${shown.distance} ${instructionFor(shown.step, text)}`}
    >
      <div className="guidance-steps">
        {leaving && (
          <div key={`out:${leaving.key}`} className="guidance-step leaving" aria-hidden>
            <StepLines shown={leaving} size={glyphSize} text={text} />
          </div>
        )}
        <div key={shown.key} className={`guidance-step${leaving ? ' entering' : ''}`}>
          <StepLines shown={shown} size={glyphSize} text={text} />
        </div>
      </div>
      <div className={`guidance-then-slot${shown.then && !closing ? ' open' : ''}`}>
        <div className="guidance-then-clip">
          <div className="guidance-then">
            <span>{text.then}</span>
            {thenShown && <ManeuverGlyph step={thenShown} size={30} />}
          </div>
        </div>
      </div>
    </article>
  )
  // On the Dashboard the card sits in a slot that grows into the column as it comes and shrinks as it goes.
  if (variant === 'widget') {
    return <div className={`widget-slot${shown && !closing ? ' open' : ''}`}>{card}</div>
  }
  return card
}

function StepLines({ shown, size, text }: { shown: Shown; size: number; text: Strings }) {
  return (
    <>
      <ManeuverGlyph step={shown.step} size={size} />
      <div>
        <div className="guidance-distance">{shown.distance}</div>
        <div className="guidance-instruction">{instructionFor(shown.step, text)}</div>
      </div>
    </>
  )
}
