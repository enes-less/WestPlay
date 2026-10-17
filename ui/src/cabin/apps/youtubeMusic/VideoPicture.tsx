import { useEffect, useRef } from 'react'
import { isInHost } from '../../../shared/host'
import { useCabin } from '../../state/store'
import { followArea } from '../youtube/youtube'
import { youtubeMusic, useYouTubeMusic } from './youtubeMusic'

/**
 * The music video's picture, in Now Playing (its Video side): YouTube Music's own, placed under the page by the
 * host, the CarPlay area cut open over this element (Shell.tsx), its rounding included. Placed again whenever
 * it may have moved (Now Playing rising, the window's size); gone as it goes, or as the screen is handed to the
 * truck. Outside WestPlay (a plain browser), the cover stands in.
 */
export function VideoPicture({ cover }: { cover: string | undefined }) {
  const frame = useRef<HTMLSpanElement>(null)
  const handedOver = useCabin((state) => state.truckScreen)
  // A sheet (Up Next, Lyrics, Related) or the ⋮ menu over Now Playing: the picture lies under the page, so the
  // hole would show it through them. Put away while one is open; the cover stands in.
  const covered = useYouTubeMusic((state) => state.panel !== null || state.menu !== null)
  useEffect(() => {
    const element = frame.current
    if (!element || handedOver || covered || !isInHost()) return
    const place = () => {
      const area = document.querySelector('.carplay')?.getBoundingClientRect()
      const box = element.getBoundingClientRect()
      if (!area || box.width === 0) return
      // Its rounding in screen pixels (the CarPlay area's content is zoomed to the screen's scale).
      const zoom = box.width / element.offsetWidth
      const radius = parseFloat(getComputedStyle(element).borderTopLeftRadius) * zoom
      useYouTubeMusic.setState({
        hole: { x: box.left - area.left, y: box.top - area.top, width: box.width, height: box.height, radius },
      })
      const scale = window.devicePixelRatio
      youtubeMusic({
        action: 'show',
        x: Math.round(box.left * scale),
        y: Math.round(box.top * scale),
        width: Math.round(box.width * scale),
        height: Math.round(box.height * scale),
      })
    }
    const following = followArea(place, element)
    // Now Playing rises in (a transform, which no size change tells of): placed again as it settles.
    const settling = [120, 380, 700].map((ms) => setTimeout(place, ms))
    return () => {
      following()
      settling.forEach(clearTimeout)
      useYouTubeMusic.setState({ hole: null })
      youtubeMusic({ action: 'hide' })
    }
  }, [handedOver, covered])
  return (
    <span ref={frame} className="ytm-video">
      {cover && <img src={cover} alt="" draggable={false} />}
    </span>
  )
}
