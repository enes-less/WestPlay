import { useEffect, useRef, useState } from 'react'
import { formatClock } from './time'
import { useCabin } from '../../state/store'
import { useText } from '../../state/text'
import {
  BackGlyph,
  CaptionsGlyph,
  CheckGlyph,
  NextGlyph,
  PauseGlyph,
  PlayGlyph,
  PreviousGlyph,
  QueueGlyph,
  ReplayGlyph,
  SettingsGlyph,
  VolumeGlyph,
} from './glyphs'
import { Thumbnail } from './YouTubeApp'
import {
  closePlayer,
  comingUp,
  command,
  followArea,
  leavePlayer,
  nextVideo,
  openChannel,
  play,
  previousVideo,
  QUALITIES,
  seekBy,
  setAudioTrack,
  setCaptions,
  setQuality,
  setRate,
  setStableVolume,
  setVoiceBoost,
  setVolume,
  toggleMute,
  useYouTube,
} from './youtube'

/** How long the controls stay after the last touch, ms. */
const CONTROLS_MS = 3000
/** Two taps this close are a double tap, ms. */
const DOUBLE_TAP_MS = 300
/** How long the "10 seconds" stays after the last tap, during which each tap adds 10 more, ms. */
const SKIP_MS = 800
const SKIP_SECONDS = 10

type Side = 'back' | 'forward'

/**
 * YouTube's player, full screen over the whole CarPlay area (only the truck's own strip beside it), as on
 * YouTube's TV app. The video itself is YouTube's, in its own WebView under this page (the host places it
 * where the CarPlay area is: `show`); this draws only what's over it: a cover with the video's thumbnail
 * until it plays, and YouTube's new controls, each in its own translucent shape, on a touch.
 */
export function Player() {
  const open = useYouTube((store) => store.open)
  const app = useCabin((state) => state.navigation.route)
  const handedOver = useCabin((state) => state.truckScreen)
  // Going elsewhere (a shortcut, the Dashboard's widget) or handing the screen to the truck leaves the player;
  // the video plays on, and YouTube opens on it again.
  useEffect(() => {
    if (open && (handedOver || app.kind !== 'app' || app.app !== 'youtube')) leavePlayer()
  }, [open, app, handedOver])
  return open ? <OpenPlayer /> : null
}

function OpenPlayer() {
  const text = useText()
  const current = useYouTube((store) => store.current)
  const state = useYouTube((store) => store.state)
  const upNext = useYouTube((store) => store.upNext)
  const playlist = useYouTube((store) => store.queue)
  const coming = comingUp({ queue: playlist, upNext, current })
  const [controls, setControls] = useState(true)
  const [queue, setQueue] = useState(false)
  // The captions' or the quality's choices, open over the controls.
  const [menu, setMenu] = useState<Menu | null>(null)
  const touched = useRef(0)
  // Taps on the picture: the last one (its side, and the single tap's action waiting for a second), and the
  // seconds skipped by double taps, shown on their side.
  const lastTap = useRef<{ at: number; side: Side | null; single?: ReturnType<typeof setTimeout> }>({
    at: -Infinity,
    side: null,
  })
  const [skip, setSkip] = useState<{ side: Side; seconds: number; at: number } | null>(null)
  useEffect(() => {
    if (!skip) return
    const timer = setTimeout(() => setSkip(null), SKIP_MS)
    return () => clearTimeout(timer)
  }, [skip])
  useEffect(() => () => clearTimeout(lastTap.current.single), [])

  // The video where the CarPlay area is, in the frame's pixels; again if the frame changes.
  useEffect(() => {
    const place = () => {
      const area = document.querySelector('.carplay')?.getBoundingClientRect()
      if (!area) return
      const scale = window.devicePixelRatio
      command({
        action: 'show',
        x: Math.round(area.left * scale),
        y: Math.round(area.top * scale),
        width: Math.round(area.width * scale),
        height: Math.round(area.height * scale),
      })
    }
    return followArea(place)
  }, [])

  const mine = state !== null && state.video === current?.video
  // Started: it has played once (the cover goes for good).
  const [started, setStarted] = useState<string | null>(null)
  if (mine && state.state === 1 && started !== current?.video) setStarted(current?.video ?? null)
  const playing = mine && state.state === 1
  const ended = mine && state.state === 0
  const buffering = mine && started === current?.video && state.state === 3

  // The controls go after a while, unless paused, ended, or showing Up next or a menu.
  const touch = () => {
    touched.current = performance.now()
    setControls(true)
  }
  useEffect(() => {
    if (!controls || !playing || queue || menu) return
    const timer = setTimeout(() => setControls(false), CONTROLS_MS)
    return () => clearTimeout(timer)
  }, [controls, playing, queue, menu])

  // One ends: the next plays, as YouTube's autoplay.
  useEffect(() => {
    if (!ended || current?.live) return
    const next = nextVideo()
    if (next) play(next)
  }, [ended, current])

  if (!current) return null
  const cover = started !== current.video
  return (
    <div
      className={`yt-player${controls ? ' controls' : ''}`}
      onPointerDown={(event) => {
        if (event.target !== event.currentTarget) return
        // A tap off an open menu only closes it.
        if (menu) {
          setMenu(null)
          touch()
          return
        }
        // A tap on the picture shows the controls, or hides them; a double tap on its left or right third
        // goes 10 seconds back or on (each tap after it 10 more), as YouTube's. A tap on a side waits a
        // moment for a second one.
        const toggle = () => {
          if (controls && playing && performance.now() - touched.current > 300) setControls(false)
          else touch()
        }
        const box = event.currentTarget.getBoundingClientRect()
        const across = (event.clientX - box.left) / box.width
        // Sides skip only in a video that has somewhere to skip to (YouTube has it, and it isn't live).
        const skippable = mine && !state.live && !current.live
        const side: Side | null = !skippable ? null : across < 1 / 3 ? 'back' : across > 2 / 3 ? 'forward' : null
        const now = performance.now()
        const last = lastTap.current
        const skipping = side !== null && skip?.side === side && now - skip.at < SKIP_MS
        if (side && (skipping || (last.side === side && now - last.at < DOUBLE_TAP_MS))) {
          clearTimeout(last.single)
          lastTap.current = { at: -Infinity, side: null }
          seekBy(side === 'back' ? -SKIP_SECONDS : SKIP_SECONDS)
          setSkip({ side, seconds: (skipping ? skip.seconds : 0) + SKIP_SECONDS, at: now })
          return
        }
        clearTimeout(last.single)
        lastTap.current = { at: now, side, single: side ? setTimeout(toggle, DOUBLE_TAP_MS) : undefined }
        if (!side) toggle()
      }}
      onPointerMove={() => controls && touch()}
    >
      {cover && (
        <div className="yt-cover">
          {current.thumbnail && <img src={current.thumbnail} alt="" />}
          <div className="yt-spinner large" role="progressbar" />
        </div>
      )}
      {buffering && <div className="yt-spinner large floating" role="progressbar" />}
      {skip && (
        <div key={skip.at} className={`yt-skip ${skip.side}`} aria-live="polite">
          <span className="yt-skip-arrows" aria-hidden="true">
            {skip.side === 'back' ? '◀◀' : '▶▶'}
          </span>
          {text.youtube.seconds(skip.seconds)}
        </div>
      )}

      <div className="yt-player-top">
        <button type="button" className="yt-glass yt-round big" aria-label={text.youtube.back} onClick={closePlayer}>
          <BackGlyph />
        </button>
        <div className="yt-player-title">
          {current.title}
          {current.channelId ? (
            <button
              type="button"
              className="yt-player-channel"
              onClick={() => {
                const { channelId, channel, avatar } = current
                closePlayer()
                if (channelId) openChannel({ channel: channelId, name: channel, avatar })
              }}
            >
              {current.channel}
            </button>
          ) : (
            <span>{current.channel}</span>
          )}
        </div>
      </div>

      {queue ? (
        <div className="yt-upnext">
          <Progress />
          <h4>{playlist ? playlist.title : text.youtube.upNext}</h4>
          <div className="yt-upnext-cards">
            {[current, ...coming.slice(0, 3)].map((video, index) => (
              <button
                key={video.video}
                type="button"
                className={index === 0 ? 'yt-card now' : 'yt-card'}
                onClick={() => {
                  setQueue(false)
                  if (index > 0) play(video)
                }}
              >
                <Thumbnail video={video} />
                <span className="yt-title">{video.title}</span>
                <span className="yt-sub">{index === 0 ? text.youtube.nowPlaying : video.channel}</span>
              </button>
            ))}
          </div>
        </div>
      ) : (
        <div className="yt-player-bottom">
          <Progress />
          <div className="yt-controls">
            <button
              type="button"
              className="yt-glass yt-pill"
              aria-label={text.previous}
              onClick={() => {
                touch()
                // Back to its start, or (at its start) the video before.
                const before = previousVideo()
                if ((state?.time ?? 0) > 5 || !before) command({ action: 'seek', seconds: 0 })
                else play(before)
              }}
            >
              <PreviousGlyph />
            </button>
            <button
              type="button"
              className="yt-glass yt-round big play"
              aria-label={playing ? text.pause : text.play}
              onClick={() => {
                touch()
                if (ended) command({ action: 'seek', seconds: 0 })
                command({ action: playing ? 'pause' : 'play' })
              }}
            >
              {ended ? <ReplayGlyph /> : playing ? <PauseGlyph /> : <PlayGlyph />}
            </button>
            <button
              type="button"
              className="yt-glass yt-pill"
              aria-label={text.next}
              disabled={!coming.length}
              onClick={() => {
                const next = nextVideo()
                if (next) play(next)
              }}
            >
              <NextGlyph />
            </button>
            <Volume onTouch={touch} />
            <div className="yt-glass yt-pill yt-time">
              {current.live || state?.live ? <span className="yt-badge live">{text.youtube.live}</span> : <Time />}
            </div>
            <span className="yt-spacer" />
            <button
              type="button"
              className={`yt-glass yt-pill yt-cc${mine && state.caption ? ' on' : ''}`}
              aria-label={text.youtube.captions}
              aria-pressed={menu === 'captions'}
              disabled={!mine || state.captions.length === 0}
              onClick={() => {
                touch()
                setMenu(menu === 'captions' ? null : 'captions')
              }}
            >
              <CaptionsGlyph />
            </button>
            <button
              type="button"
              className="yt-glass yt-pill yt-gear"
              aria-label={text.youtube.settings}
              aria-pressed={menu === 'settings'}
              disabled={!mine || state.qualities.length === 0}
              onClick={() => {
                touch()
                setMenu(menu === 'settings' ? null : 'settings')
              }}
            >
              <SettingsGlyph />
              {mine && sharpness(state.qualityLabel) && <span className="yt-hd">{sharpness(state.qualityLabel)}</span>}
            </button>
            <button
              type="button"
              className="yt-glass yt-pill"
              aria-label={text.youtube.upNext}
              disabled={!coming.length}
              onClick={() => {
                touch()
                setMenu(null)
                setQueue(true)
              }}
            >
              <QueueGlyph />
            </button>
          </div>
          {menu === 'captions' && <CaptionsMenu onChosen={() => setMenu(null)} />}
          {menu === 'settings' && <SettingsMenu onChosen={() => setMenu(null)} />}
        </div>
      )}
      {queue && (
        <button type="button" className="yt-upnext-dismiss" aria-label={text.close} onClick={() => setQueue(false)} />
      )}
    </div>
  )
}

/** Where the video is, between YouTube's reports: run on by the clock while it plays. */
function usePosition(): { time: number; duration: number } {
  const state = useYouTube((store) => store.state)
  const at = useYouTube((store) => store.stateAt)
  const [now, setNow] = useState(() => performance.now())
  const playing = state?.state === 1
  useEffect(() => {
    if (!playing) return
    let frame = requestAnimationFrame(function tick() {
      setNow(performance.now())
      frame = requestAnimationFrame(tick)
    })
    return () => cancelAnimationFrame(frame)
  }, [playing])
  if (!state) return { time: 0, duration: 0 }
  const run = playing ? Math.max(0, (now - at) / 1000) : 0
  return { time: Math.min(state.time + run, state.duration || Infinity), duration: state.duration }
}

/** The red bar: what's played, with its dot; a press seeks there. */
function Progress() {
  const { time, duration } = usePosition()
  const live = useYouTube((store) => store.state?.live ?? false)
  const played = duration > 0 && !live ? (time / duration) * 100 : live ? 100 : 0
  return (
    <div
      className="yt-bar"
      onPointerDown={(event) => {
        if (live || duration <= 0) return
        const bar = event.currentTarget.getBoundingClientRect()
        const at = Math.min(1, Math.max(0, (event.clientX - bar.left) / bar.width))
        command({ action: 'seek', seconds: at * duration })
      }}
    >
      <span className="yt-bar-played" style={{ width: `${played}%` }} />
      <span className="yt-bar-dot" style={{ left: `${played}%` }} />
    </div>
  )
}

function Time() {
  const { time, duration } = usePosition()
  return (
    <span>
      {formatClock(time)} / {formatClock(duration)}
    </span>
  )
}

/** The sound, the player's own for every video: muted or not, and how loud (the slider is pressed or dragged
 * to a level). */
function Volume({ onTouch }: { onTouch(): void }) {
  const text = useText()
  const volume = useYouTube((store) => store.volume)
  const muted = useYouTube((store) => store.muted)
  const shown = muted ? 0 : volume
  // Where the pointer is along the slider, as a volume.
  const at = (slider: HTMLElement, x: number) => {
    const box = slider.getBoundingClientRect()
    return Math.min(1, Math.max(0, (x - box.left) / box.width)) * 100
  }
  return (
    <div className="yt-glass yt-pill yt-volume">
      <button
        type="button"
        aria-label={text.youtube.volume}
        onClick={() => {
          onTouch()
          toggleMute()
        }}
      >
        <VolumeGlyph muted={shown === 0} />
      </button>
      <span
        className="yt-volume-slider"
        role="slider"
        aria-label={text.youtube.volume}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={shown}
        onPointerDown={(event) => {
          // Grabbed: it follows the pointer, anywhere on the page, until let go.
          const slider = event.currentTarget
          onTouch()
          setVolume(at(slider, event.clientX))
          const drag = (move: PointerEvent) => {
            const next = Math.round(at(slider, move.clientX))
            if (next !== useYouTube.getState().volume || useYouTube.getState().muted) setVolume(next)
            onTouch()
          }
          const release = () => {
            window.removeEventListener('pointermove', drag)
            window.removeEventListener('pointerup', release)
            window.removeEventListener('pointercancel', release)
          }
          window.addEventListener('pointermove', drag)
          window.addEventListener('pointerup', release)
          window.addEventListener('pointercancel', release)
        }}
      >
        <span className="yt-volume-level" style={{ width: `${shown}%` }} />
      </span>
    </div>
  )
}

type Menu = 'captions' | 'settings'

/** YouTube's word over its gear for a sharp picture ("720p60": HD; 2160p and over: 4K), or none. */
function sharpness(label: string): string | null {
  const lines = parseInt(label, 10)
  return lines >= 2160 ? '4K' : lines >= 720 ? 'HD' : null
}

/** A quality's label from YouTube's name for it, when YouTube gave none (`hd1080`: "1080p"). */
const LINES: Record<string, string> = {
  tiny: '144p',
  small: '240p',
  medium: '360p',
  large: '480p',
  hd720: '720p',
  hd1080: '1080p',
  hd1440: '1440p',
  hd2160: '2160p',
  hd2880: '2880p',
  highres: '4320p',
}

/** A list of choices, the one chosen ticked. */
function Choices<T extends string | number | null>({
  options,
  chosen,
  onChoose,
}: {
  options: { value: T; label: string }[]
  chosen: T
  onChoose(value: T): void
}) {
  return (
    <div className="yt-sheet-options">
      {options.map((option) => (
        <button
          key={String(option.value ?? 'off')}
          type="button"
          role="menuitemradio"
          aria-checked={option.value === chosen}
          onClick={() => onChoose(option.value)}
        >
          <span className="yt-sheet-tick">{option.value === chosen && <CheckGlyph />}</span>
          {option.label}
        </button>
      ))}
    </div>
  )
}

/** The captions' choices (the CC button's): off, or one of the video's tracks. */
function CaptionsMenu({ onChosen }: { onChosen(): void }) {
  const text = useText()
  const state = useYouTube((store) => store.state)
  if (!state) return null
  return (
    <div className="yt-sheet" role="menu" aria-label={text.youtube.captions}>
      <h4>{text.youtube.captions}</h4>
      <Choices
        options={[
          { value: null, label: text.youtube.off },
          ...state.captions.map((t) => ({ value: t.track, label: t.name })),
        ]}
        chosen={state.caption}
        onChoose={(track: string | null) => {
          setCaptions(track)
          onChosen()
        }}
      />
    </div>
  )
}

/** How a speed reads ("Normal", "1.5×"). */
function speedName(rate: number, normal: string): string {
  return rate === 1 ? normal : `${rate}×`
}

/**
 * The gear's settings, as YouTube's app has them: a row each, with what it is now; a row pressed opens its
 * choices (back to the rows with the arrow), a switch turns over where it is. Quality (YouTube's own, with
 * the one playing, or one the video has: chosen, it's the most for every video after, the best each has at
 * or under it); playback speed (on the videos after too); the audio track (a video with dubbed ones);
 * Stable volume and Voice boost (a video that offers them).
 */
function SettingsMenu({ onChosen }: { onChosen(): void }) {
  const text = useText()
  const state = useYouTube((store) => store.state)
  const quality = useYouTube((store) => store.quality)
  const [page, setPage] = useState<'quality' | 'speed' | 'audio' | null>(null)
  if (!state) return null
  const qualities = [
    {
      value: 'auto',
      label: `${text.youtube.auto}${quality === 'auto' && state.qualityLabel ? ` (${state.qualityLabel})` : ''}`,
    },
    ...state.qualities.map((q) => ({ value: q.quality, label: q.label || LINES[q.quality] || q.quality })),
  ]
  // The one chosen, or (chosen above what this video has) the one it plays at.
  const most = QUALITIES.indexOf(quality)
  const chosenQuality =
    quality === 'auto'
      ? 'auto'
      : (state.qualities.find((q) => QUALITIES.indexOf(q.quality) <= most)?.quality ??
        state.qualities.at(-1)?.quality ??
        'auto')
  const speeds = state.rates.map((rate) => ({ value: rate, label: speedName(rate, text.youtube.normal) }))
  const tracks = state.audioTracks.map((track) => ({ value: track.id, label: track.name }))
  const pages = {
    quality: {
      title: text.youtube.quality,
      list: (
        <Choices options={qualities} chosen={chosenQuality} onChoose={(q: string) => (setQuality(q), onChosen())} />
      ),
    },
    speed: {
      title: text.youtube.playbackSpeed,
      list: <Choices options={speeds} chosen={state.rate} onChoose={(rate: number) => (setRate(rate), onChosen())} />,
    },
    audio: {
      title: text.youtube.audioTrack,
      list: (
        <Choices
          options={tracks}
          chosen={state.audioTrack ?? ''}
          onChoose={(id: string) => (setAudioTrack(id), onChosen())}
        />
      ),
    },
  }
  if (page) {
    return (
      <div className="yt-sheet" role="menu" aria-label={pages[page].title}>
        <button type="button" className="yt-sheet-back" onClick={() => setPage(null)}>
          <BackGlyph />
          {pages[page].title}
        </button>
        {pages[page].list}
      </div>
    )
  }
  const row = (key: 'quality' | 'speed' | 'audio', label: string, value: string) => (
    <button key={key} type="button" className="yt-sheet-row" onClick={() => setPage(key)}>
      <span>{label}</span>
      <span className="yt-sheet-value">{value}</span>
      <span className="yt-sheet-chevron" aria-hidden="true">
        ›
      </span>
    </button>
  )
  const toggle = (key: string, label: string, on: boolean, turn: (on: boolean) => void) => (
    <button key={key} type="button" role="switch" aria-checked={on} className="yt-sheet-row" onClick={() => turn(!on)}>
      <span>{label}</span>
      <span className={on ? 'yt-switch on' : 'yt-switch'} aria-hidden="true" />
    </button>
  )
  const playing = state.audioTracks.find((track) => track.id === state.audioTrack)
  return (
    <div className="yt-sheet" role="menu" aria-label={text.youtube.settings}>
      <div className="yt-sheet-options">
        {state.qualities.length > 0 &&
          row('quality', text.youtube.quality, qualities.find((q) => q.value === chosenQuality)?.label ?? '')}
        {state.rates.length > 1 && row('speed', text.youtube.playbackSpeed, speedName(state.rate, text.youtube.normal))}
        {state.audioTracks.length > 1 && row('audio', text.youtube.audioTrack, playing?.name ?? '')}
        {state.stableVolume !== null &&
          toggle('stable', text.youtube.stableVolume, state.stableVolume, setStableVolume)}
        {state.voiceBoost !== null && toggle('voice', text.youtube.voiceBoost, state.voiceBoost, setVoiceBoost)}
      </div>
    </div>
  )
}
