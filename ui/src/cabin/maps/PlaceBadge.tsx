import type { Appearance } from '../state/appearance'
import { useLook } from '../state/hooks'
import type { DestinationKind } from './places'
import { drawPlaceIcon } from './placeIcons'

/** Each kind's badge as an image, by kind and appearance: drawn once, as the map draws its icons. */
const drawn = new Map<string, string>()

function badgeImage(kind: Exclude<DestinationKind, 'city'>, appearance: Appearance): string {
  const key = `${kind}:${appearance}`
  let url = drawn.get(key)
  if (url === undefined) {
    const { width, height, data } = drawPlaceIcon(kind, null, appearance, 2, '')
    const canvas = document.createElement('canvas')
    canvas.width = width
    canvas.height = height
    canvas.getContext('2d')?.putImageData(new ImageData(new Uint8ClampedArray(data), width, height), 0, 0)
    url = canvas.toDataURL()
    drawn.set(key, url)
  }
  return url
}

/**
 * A place's badge in Destinations' lists, the same as on the map (placeIcons.ts); a city's, which the map
 * names rather than marks, a slate square with a skyline.
 */
export function PlaceBadge({ kind }: { kind: DestinationKind }) {
  const { appearance } = useLook()
  if (kind === 'city') {
    return (
      <span className="place-badge city" aria-hidden>
        <svg viewBox="0 0 24 24">
          <path d="M4 20V9h5V4h7v7h4v9ZM6.5 11.5v2h2v-2Zm4-5v2h2v-2Zm0 4v2h2v-2Zm0 4v2h2v-2Zm6 0v2h2v-2Z" />
        </svg>
      </span>
    )
  }
  return <img className="place-badge" src={badgeImage(kind, appearance)} alt="" />
}
