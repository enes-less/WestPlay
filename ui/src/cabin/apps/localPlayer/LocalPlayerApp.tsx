import React, { useEffect, useRef, useState } from 'react'
import { useCabin, useLive } from '../../state/store'
import { useText } from '../../state/text'
import { useLocalPlayer } from './localPlayer'
import './localPlayer.css'

export function LocalPlayerApp() {
  const [paused, setPaused] = useState(false)

  const url = useLocalPlayer((state) => state.url)
  const setUrl = useLocalPlayer((state) => state.setUrl)
  const setTime = useLocalPlayer((state) => state.setTime)

  const inputRef = useRef<HTMLInputElement>(null)
  const videoRef = useRef<HTMLVideoElement>(null)

  const text = useText().localPlayer

  useEffect(() => pauseWithGame(), [])
  useEffect(() => {
    videoRef.current!.currentTime = useLocalPlayer.getState().time
    videoRef.current!.addEventListener('timeupdate', () => {
      setTime(videoRef.current!.currentTime)
    })
  }, [])
  useEffect(() => {
    if (paused) {
      videoRef.current?.pause()
    } else {
      videoRef.current?.play()
    }
  }, [paused])
  // Pause video when game is paused
  const pauseWithGame = () => {
    const game = useLive.subscribe((live, before) => {
      const wasPaused = (before.telemetry?.paused ?? true) || !before.gameFocused
      const paused = (live.telemetry?.paused ?? true) || !live.gameFocused

      if (paused === wasPaused) {
        return
      }

      if (paused && !wasPaused) {
        setPaused(true)
      } else if (!paused && wasPaused) {
        setPaused(false)
      }
    })

    const power = useCabin.subscribe((cabin, before) => {
      const wasOff = before.power.phase === 'off'
      const isOff = cabin.power.phase === 'off'

      if (isOff === wasOff) {
        return
      }

      if (isOff && !wasOff) {
        setPaused(true)
      } else if (!isOff && wasOff) {
        setPaused(false)
      }
    })

    return () => {
      game()
      power()
    }
  }

  const handleFileChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return

    if (!file.type.startsWith('video/')) {
      e.target.value = ''
      return
    }

    if (url) {
      URL.revokeObjectURL(url)
    }

    setUrl(URL.createObjectURL(file))
  }

  // Right click to select video file
  const selectVideo = (e: React.MouseEvent<HTMLElement>) => {
    e.preventDefault()
    inputRef.current?.click()
  }

  return (
    <section className="app-panel local-player">
      <input ref={inputRef} hidden type="file" accept="video/*" onChange={handleFileChange} />
      <video ref={videoRef} src={url} loop controls autoPlay onContextMenu={selectVideo}></video>
      {!url && (
        <p className="open-hint" onContextMenu={selectVideo}>
          {text.openHint}
        </p>
      )}
    </section>
  )
}
