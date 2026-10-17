import { useEffect, useRef, useState } from 'react'
import { useWallpapers } from '../state/looks'
import { allWallpaperSources, prepareWallpaper } from '../wallpapers'

/** Must match --motion-duration in shell.css: how long the outgoing wallpaper stays under the incoming one. */
const TRANSITION_MS = 650

interface Layer {
  src: string
  key: number
}

/**
 * The wallpaper behind everything. A change of image crossfades: the incoming layer fades in on top of
 * the outgoing one, which is removed once the fade is over. Every wallpaper is prepared as a screen-sized
 * bitmap up front so the fade never stutters.
 */
export function Wallpaper({ src }: { src: string }) {
  const [layers, setLayers] = useState<Layer[]>([{ src, key: 0 }])
  const prepared = usePreparedWallpapers()

  // A new image starts a crossfade. Adjusting state while rendering (instead of in an effect) is React's
  // pattern for state that follows a prop: the new layer is there in the very first render with it.
  const top = layers[layers.length - 1]
  if (top.src !== src) {
    setLayers([top, { src, key: top.key + 1 }])
  }

  useEffect(() => {
    if (layers.length < 2) return
    const timer = setTimeout(() => setLayers((current) => current.slice(-1)), TRANSITION_MS)
    return () => clearTimeout(timer)
  }, [layers])

  return (
    <div className="wallpaper-stack">
      {layers.map((layer) => (
        <img key={layer.key} className="wallpaper" src={prepared[layer.src] ?? layer.src} alt="" />
      ))}
    </div>
  )
}

/** Screen-sized bitmaps of every wallpaper image, by source; filled in as they become ready (each once, the
 * player's own too, as they're added). */
function usePreparedWallpapers(): Record<string, string> {
  const wallpapers = useWallpapers()
  const [prepared, setPrepared] = useState<Record<string, string>>({})
  const started = useRef(new Set<string>())
  const urls = useRef<string[]>([])
  const alive = useRef(true)
  const sources = allWallpaperSources(wallpapers)
  const key = sources.join('\n')

  useEffect(() => {
    for (const source of sources) {
      if (started.current.has(source)) continue
      started.current.add(source)
      prepareWallpaper(source, window.innerWidth, window.innerHeight)
        .then((url) => {
          urls.current.push(url)
          if (alive.current) setPrepared((current) => ({ ...current, [source]: url }))
        })
        .catch((error: unknown) => console.warn(`cannot prepare wallpaper ${source}`, error))
    }
    // Keyed by the sources themselves: each is prepared once, whatever came before it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key])
  useEffect(() => {
    alive.current = true
    const made = urls.current
    return () => {
      alive.current = false
      made.forEach((url) => URL.revokeObjectURL(url))
    }
  }, [])
  return prepared
}
