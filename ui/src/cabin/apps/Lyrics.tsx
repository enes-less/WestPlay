import { type ReactNode, useEffect, useRef } from 'react'
import type { Lyrics } from '../../shared/host'
import { lineAt } from './lyricsTime'

/** How long the reader's own scrolling holds the words where they put them before they follow the song again. */
const HELD = 4000

/**
 * A song's words in Now Playing (Apple Music's, Spotify's; each app styles them under its own `className`):
 * synced, each line lit as it's sung (`now`; those sung `sung`), the one sung kept a third of the way down, and
 * pressed, the song plays from it; else the words as they are. `before` and `after` go above and below them (a
 * note, the credits).
 */
export function LyricsLines({
  lyrics,
  position,
  onSeek,
  className,
  before,
  after,
}: {
  lyrics: Lyrics
  position: number
  onSeek(seconds: number): void
  className: string
  before?: ReactNode
  after?: ReactNode
}) {
  const box = useRef<HTMLDivElement>(null)
  const held = useRef(0)
  const first = useRef(true)
  const current = lyrics.synced ? lineAt(lyrics.lines, position) : -1

  // The line sung, followed (on this box only: never its parents'). At once as the words open, smoothly after.
  useEffect(() => {
    const scroller = box.current
    if (!scroller || !lyrics.synced || performance.now() < held.current) return
    const line = scroller.querySelectorAll<HTMLElement>('.lyric')[Math.max(0, current)]
    if (!line) return
    const top = line.offsetTop - scroller.clientHeight * 0.3
    scroller.scrollTo({ top: Math.max(0, top), behavior: first.current ? 'auto' : 'smooth' })
    first.current = false
  }, [current, lyrics])

  const hold = () => {
    held.current = performance.now() + HELD
  }

  return (
    <div
      ref={box}
      className={`lyrics ${className}`}
      dir={lyrics.rtl ? 'rtl' : undefined}
      onWheel={hold}
      onPointerDown={hold}
    >
      {before}
      {lyrics.lines.map((line, i) => {
        const state = !lyrics.synced ? '' : i === current ? ' now' : i < current ? ' sung' : ''
        const name = `lyric${state}${line.stanza ? ' stanza' : ''}`
        return lyrics.synced ? (
          <button
            key={i}
            type="button"
            className={name}
            onClick={() => {
              held.current = 0
              onSeek(line.start)
            }}
          >
            {line.text}
          </button>
        ) : (
          <p key={i} className={name}>
            {line.text}
          </p>
        )
      })}
      {after}
    </div>
  )
}
