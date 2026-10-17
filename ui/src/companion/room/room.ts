import { createContext, type CSSProperties, useContext, useEffect, useState } from 'react'
import { count, fill, type Words, wordsFor } from '../onboarding/words'
import { type GameInfo, gameRunning } from '../setup'
import type { MapStage } from '../useHostStatus'
import type { Banner } from './TruckScreen'

/* What the room's parts share that isn't a part: the language its words are in, a part's place in the order
   things arrive in, a headline's letters, a banner from a note, and the hooks for the game and sign-ins. */

/** The language a room's words are in, and its Back button's name. */
export const RoomText = createContext<{ locale: string; back: string }>({ locale: 'en-US', back: 'Back' })

export const n = (place: number) => ({ '--n': place }) as CSSProperties

/** A headline's words (each kept whole on a line) and their letters (as the language writes them: a letter
 * with its marks is one), with where each word's letters start in the whole. Words where the language has
 * them: Japanese and Chinese, without spaces, are split where a line may break. */
export function headline(title: string, locale: string): { letters: string[]; start: number }[] {
  const letters = (text: string) =>
    [...new Intl.Segmenter(locale, { granularity: 'grapheme' }).segment(text)].map((part) => part.segment)
  const words: { letters: string[]; start: number }[] = []
  let start = 0
  for (const { segment } of new Intl.Segmenter(locale, { granularity: 'word' }).segment(title)) {
    const parts = letters(segment)
    // Spaces and punctuation go with the word before them (no line starts with them).
    if (words.length && !/\p{L}|\p{N}/u.test(segment)) {
      words[words.length - 1].letters.push(...parts)
    } else {
      words.push({ letters: parts, start })
    }
    start += parts.length
  }
  return words
}

/** A banner on the screen from a note's words. */
export function banner(note: { title: string; text: string }, busy = false): Banner {
  return { title: note.title, detail: note.text, busy }
}

/** Whether the game is running, looked at every couple of seconds. */
export function useGameRunning(): boolean | null {
  const [running, setRunning] = useState<boolean | null>(null)
  useEffect(() => {
    let active = true
    const look = () => gameRunning().then((now) => active && setRunning(now))
    look()
    const timer = window.setInterval(look, 2000)
    return () => {
      active = false
      window.clearInterval(timer)
    }
  }, [])
  return running
}

/** How long a sign-out has to be done before its button can be pressed again (it failed, or took too long). */
export const SIGN_OUT_WAIT = 20000

/**
 * A sign-out under way: it takes a while (the service's page loads again, signed out), so its button shows
 * it's working, and can't be pressed again, until the service says it's signed out, or `SIGN_OUT_WAIT`
 * passes (then it can be tried again).
 */
export function useSignOut(signedIn: boolean, signOut: () => Promise<void>): { signingOut: boolean; start(): void } {
  const [signingOut, setSigningOut] = useState(false)
  // Signed out: done.
  if (signingOut && !signedIn) setSigningOut(false)
  useEffect(() => {
    if (!signingOut) return
    const timer = window.setTimeout(() => setSigningOut(false), SIGN_OUT_WAIT)
    return () => window.clearTimeout(timer)
  }, [signingOut])
  return {
    signingOut,
    start: () => {
      setSigningOut(true)
      signOut().catch(() => setSigningOut(false))
    },
  }
}

/** How long a step waits to know whether its service is signed in before taking it as signed out (offline,
 * say), so it never holds the player up. */
export const CHECK_WAIT = 15000

/**
 * Whether a sign-in step is still finding out if its service is signed in (`known`: the status says, one way
 * or the other): it asks once (`check`: the service run, hidden), and gives up waiting after `CHECK_WAIT`.
 */
export function useChecking(known: boolean, check: () => Promise<void>): boolean {
  const [waited, setWaited] = useState(false)
  useEffect(() => {
    check().catch(() => setWaited(true))
    const timer = window.setTimeout(() => setWaited(true), CHECK_WAIT)
    return () => window.clearTimeout(timer)
    // Asked once, as the step shows.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  return !known && !waited
}

/** The onboarding's words, and their language's game code (`tr_tr`). */
export const WordsContext = createContext<{ words: Words; code: string }>({ words: wordsFor('en_us'), code: 'en_us' })

export function useWords(): Words {
  return useContext(WordsContext).words
}

/** The map's build, its stages in order, and the share of the whole each one ends at (the builder's own:
 * crates/mapdata export.rs) and about how long it takes (a whole map, on a fast PC). */
export const MAP_STAGES: { stage: MapStage; ends: number; seconds: number }[] = [
  { stage: 'reading', ends: 0.15, seconds: 8 },
  { stage: 'drawing', ends: 0.6, seconds: 30 },
  { stage: 'tiles', ends: 0.95, seconds: 20 },
  { stage: 'saving', ends: 1, seconds: 2 },
]

/**
 * How far the build is, to show: the builder says where each stage starts, and between those the share
 * glides on at the pace a stage takes, slowing as it goes; it never reaches the next stage before the
 * builder does.
 */
export function useGlidingProgress(done: number | null, stage: MapStage | null): number {
  const [shown, setShown] = useState(done ?? 0)
  useEffect(() => {
    if (done === null || stage === null) return
    const { ends, seconds } = MAP_STAGES.find((known) => known.stage === stage) ?? MAP_STAGES[0]
    const ceiling = done + (ends - done) * 0.94
    // Each 200 ms, a share of what's left: 95% of the way in about the stage's time.
    const rate = 1 - Math.exp(-0.2 / (seconds / 3))
    const timer = window.setInterval(
      () => setShown((now) => Math.max(now, done) + (ceiling - Math.max(now, done)) * rate),
      200,
    )
    return () => window.clearInterval(timer)
  }, [done, stage])
  return Math.max(shown, done ?? 0)
}

export function gameSummary(words: Words, game: GameInfo): string {
  const { summary } = words
  const version = game.version ? fill(summary.version, { version: game.version }) : summary.versionUnknown
  return `${version} · ${summary[game.foundBy]} · ${count(words, summary.dlcs, game.dlcs)}`
}

/** An error from setup, in the player's language if it's one setup names (`notTheGame`…). */
export function explain(words: Words, error: unknown): string {
  const text = String(error)
  const known: Record<string, string> = {
    notTheGame: words.game.notTheGame,
    gameGone: words.game.gameGone,
    noPlugin: words.install.noPlugin,
    noDocuments: words.install.noDocuments,
  }
  return known[text] ?? text
}
