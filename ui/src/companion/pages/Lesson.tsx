import { type PointerEvent, useEffect, useRef, useState } from 'react'
import { Button, Panel } from '../room/parts'
import { useWords } from '../room/room'
import { GLOW, useScreen } from '../room/screen'
import { useKeyName } from '../keys'
import type { KeyCode } from '../useSettings'
import type { CompanionWords } from './words'

type Step = 'hold' | 'open' | 'home' | 'dashboard' | 'letGo' | 'done'

/** The light the screen casts on each place the player goes. */
function glowAt(route: string): string {
  const app = route.startsWith('app:') ? route.slice(4) : null
  if (route === 'dashboard') return GLOW.dashboard
  if (app && app in GLOW) return GLOW[app as keyof typeof GLOW]
  return app ? GLOW.music : GLOW.home
}

/**
 * The tutorial's first part, in the screen, full size: how WestPlay is used in the cab. The player holds
 * their key (the screen takes their mouse), opens an app, goes back Home, on to the Dashboard, and lets go;
 * each step moves on when it's done, its words under the screen and a banner on it. Let go too soon, and the
 * screen asks for the key again.
 */
export function Lesson({
  cw,
  interactionKey,
  held,
  route,
  next,
}: {
  cw: CompanionWords
  interactionKey: KeyCode
  held: boolean
  /** Where WestPlay is (`home`, `dashboard`, `app:music`), as it says. */
  route: string
  next(): void
}) {
  const screen = useScreen()
  const words = useWords()
  const t = cw.tutorial
  const keyName = useKeyName(interactionKey, words)
  const fill = (text: string) => text.replaceAll('{key}', keyName)
  const [step, setStep] = useState<Step>('hold')

  // Each step done, the next: as the player does it.
  if (step === 'hold' && held) setStep('open')
  else if (step === 'open' && held && route.startsWith('app:')) setStep('home')
  else if (step === 'home' && route === 'home') setStep('dashboard')
  else if (step === 'dashboard' && route === 'dashboard') setStep('letGo')
  else if (step === 'letGo' && !held) setStep('done')

  // It starts on Home.
  useEffect(() => {
    screen.go('home')
    // Once, as it starts.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  useEffect(() => {
    screen.glow(glowAt(route))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [route])
  const letGoTooSoon = !held && (step === 'open' || step === 'home' || step === 'dashboard')
  const shown = letGoTooSoon ? t.holdAgain : t[step].banner
  const bannerKey = `${fill(shown.title)}|${shown.text}`
  useEffect(() => {
    screen.banner({ title: fill(shown.title), detail: shown.text })
    return () => screen.banner(null)
    // Keyed by its words.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [bannerKey])

  const lesson = t[step]
  return (
    <Panel
      place="below"
      title={fill(lesson.title)}
      sub={fill(lesson.sub)}
      action={step === 'done' ? <Button onClick={next}>{t.next}</Button> : undefined}
    />
  )
}

/**
 * Over the screen while the player's key is held: their mouse, as a cursor on the glass that follows it
 * exactly (set on the element as the mouse moves: no easing, nothing drawn again but it), and their clicks
 * pressed on WestPlay (in the screen's own pixels). The window keeps the keyboard, to know the key's let go.
 */
export function TouchLayer({ active, press }: { active: boolean; press(at: { x: number; y: number }): void }) {
  const cursor = useRef<HTMLDivElement>(null)
  const at = (event: PointerEvent<HTMLDivElement>) => {
    const box = event.currentTarget.getBoundingClientRect()
    return { x: ((event.clientX - box.left) / box.width) * 1280, y: ((event.clientY - box.top) / box.height) * 720 }
  }
  const place = (event: PointerEvent<HTMLDivElement>) => {
    const { x, y } = at(event)
    const element = cursor.current
    if (!element) return
    element.style.transform = `translate(${x}px, ${y}px)`
    element.dataset.visible = ''
  }
  const pressing = (on: boolean) => {
    if (on) cursor.current?.setAttribute('data-pressing', '')
    else cursor.current?.removeAttribute('data-pressing')
  }
  useEffect(() => {
    if (!active) cursor.current?.removeAttribute('data-visible')
  }, [active])
  return (
    <div
      className="ts-touch"
      data-active={active || undefined}
      onPointerMove={(event) => active && place(event)}
      onPointerLeave={() => cursor.current?.removeAttribute('data-visible')}
      onPointerDown={(event) => {
        if (!active || event.button !== 0) return
        event.preventDefault()
        place(event)
        pressing(true)
        press(at(event))
      }}
      onPointerUp={() => pressing(false)}
    >
      <div ref={cursor} className="ts-cursor" data-live>
        <i />
      </div>
    </div>
  )
}
