import { useState } from 'react'
import { Button, Note, Panel } from '../room/parts'
import { n, useWords } from '../room/room'
import { GLOW, ON_SCREEN, useScreenAt, useTour } from '../room/screen'
import { finishSetup } from '../setup'
import type { CompanionWords, SectionId } from './words'

/**
 * The tutorial's second part, in the room: each section of the companion in turn, open, with a card under
 * the list saying what it's for. Its last Next ends the tour.
 */
export function TourCard({
  cw,
  section,
  last,
  next,
}: {
  cw: CompanionWords
  section: SectionId
  last: boolean
  next(): void
}) {
  return (
    <aside key={section} className="cp-tour" aria-live="polite">
      <b className="ob-in" style={n(0)}>
        {cw.sections[section]}
      </b>
      <p className="ob-in" style={n(1)}>
        {cw.tutorial.tour[section]}
      </p>
      <div className="ob-in" style={n(2)}>
        <Button small onClick={next}>
          {last ? cw.tutorial.finish : cw.tutorial.next}
        </Button>
      </div>
    </aside>
  )
}

/**
 * The tutorial's end, the screen back at centre stage on the Dashboard: what to do in the game (start it,
 * turn the mod on, get in the truck), the onboarding's last words. Done, setup's finished and the companion
 * opens.
 */
export function Launch({ done }: { done(): void }) {
  const [error, setError] = useState<string | null>(null)
  const words = useWords()
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
      title={words.launch.title}
      action={
        <Button
          onClick={() => {
            setError(null)
            finishSetup()
              .then(done)
              .catch((e: unknown) => setError(String(e)))
          }}
        >
          {words.launch.done}
        </Button>
      }
    >
      <ol className="ob-steps ob-in" style={n(3)}>
        {words.launch.steps.map((step, at) => (
          <li key={at}>
            <b>
              <i>{new Intl.NumberFormat(words.locale).format(at + 1)}</i>
              {step.title}
            </b>
            {step.text}
          </li>
        ))}
      </ol>
      {error && <Note tone="bad">{error}</Note>}
    </Panel>
  )
}
