import { useEffect, useState } from 'react'
import { go } from '../../state/store'
import { useText } from '../../state/text'
import { FullScreenGlyph, NextGlyph, PauseGlyph, PlayGlyph, PreviousGlyph } from './glyphs'
import { OverVideo, VideoHole } from './VideoHole'
import { comingUp, command, nextVideo, play, positionNow, previousVideo, useYouTube } from './youtube'

/** How long the controls stay after the last press, ms. */
const CONTROLS_MS = 3000

/**
 * The Dashboard's Now Playing while a video plays: only the video, filling the card. A press brings up its
 * controls over it (previous, play or pause, next, and full screen), gone again after a while.
 */
export function VideoWidget() {
  const text = useText()
  const current = useYouTube((store) => store.current)
  const playing = useYouTube((store) => store.state?.state === 1 && store.state.video === store.current?.video)
  const hasNext = useYouTube((store) => comingUp(store).length > 0)
  const [controls, setControls] = useState(false)
  const [touched, setTouched] = useState(0)
  const touch = () => {
    setControls(true)
    setTouched((n) => n + 1)
  }
  useEffect(() => {
    if (!controls || !playing) return
    const timer = setTimeout(() => setControls(false), CONTROLS_MS)
    return () => clearTimeout(timer)
  }, [controls, playing, touched])

  return (
    <article className="widget now-playing yt-widget" aria-label={current?.title}>
      <VideoHole className="yt-widget-video" crop />
      <OverVideo>
        <div
          className={controls ? 'yt-widget-over shown' : 'yt-widget-over'}
          onPointerDown={(event) => {
            // A press on the picture brings the controls up, or puts them away.
            if (event.target !== event.currentTarget) return
            if (controls) setControls(false)
            else touch()
          }}
        >
          {controls && current && (
            <>
              <span className="yt-widget-title">{current.title}</span>
              <div className="yt-widget-controls">
                <button
                  type="button"
                  aria-label={text.previous}
                  onClick={() => {
                    touch()
                    const before = previousVideo()
                    if (positionNow() > 5 || !before) command({ action: 'seek', seconds: 0 })
                    else play(before, false)
                  }}
                >
                  <PreviousGlyph />
                </button>
                <button
                  type="button"
                  className="main"
                  aria-label={playing ? text.pause : text.play}
                  onClick={() => {
                    touch()
                    command({ action: playing ? 'pause' : 'play' })
                  }}
                >
                  {playing ? <PauseGlyph /> : <PlayGlyph />}
                </button>
                <button
                  type="button"
                  aria-label={text.next}
                  disabled={!hasNext}
                  onClick={() => {
                    touch()
                    const next = nextVideo()
                    if (next) play(next, false)
                  }}
                >
                  <NextGlyph />
                </button>
              </div>
              <button
                type="button"
                className="yt-widget-full"
                aria-label={text.youtube.fullScreen}
                onClick={() => {
                  // YouTube, opening on the video.
                  useYouTube.setState({ returnToPlayer: true })
                  go({ type: 'openApp', app: 'youtube' })
                }}
              >
                <FullScreenGlyph />
              </button>
            </>
          )}
        </div>
      </OverVideo>
    </article>
  )
}
