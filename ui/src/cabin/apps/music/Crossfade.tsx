import { type ReactNode, useEffect, useState } from 'react'
import { TRACK_CHANGE_MS } from './track'
import './trackChange.css'

interface Layer {
  value: string | null
  id: number
}

/**
 * Crossfades whatever `render` draws for `value` whenever `value` changes: the new layer fades in while the
 * old one fades out, then the old one is removed. Used for the cover and the background, so they change
 * together with the track's text. `className` positions the stack; each layer fills it.
 */
export function Crossfade({
  value,
  className,
  render,
}: {
  value: string | null
  className: string
  render(value: string | null): ReactNode
}) {
  const [layers, setLayers] = useState<Layer[]>([{ value, id: 0 }])

  // Adjusting state during render: a new value starts a crossfade (at most two layers; a fast change
  // replaces the outgoing one).
  const top = layers[layers.length - 1]
  if (top.value !== value) {
    setLayers([top, { value, id: top.id + 1 }])
  }

  useEffect(() => {
    if (layers.length < 2) return
    const timer = setTimeout(() => setLayers((current) => current.slice(-1)), TRACK_CHANGE_MS)
    return () => clearTimeout(timer)
  }, [layers])

  return (
    <div className={className}>
      {layers.map((layer, index) => (
        <div
          key={layer.id}
          className={layers.length < 2 ? 'crossfade-layer' : index === 0 ? 'crossfade-layer out' : 'crossfade-layer in'}
        >
          {render(layer.value)}
        </div>
      ))}
    </div>
  )
}
