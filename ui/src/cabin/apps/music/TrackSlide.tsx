import type { ReactNode } from 'react'
import './trackChange.css'
import type { ShownTrack, TrackTransition } from './useTrackTransition'

/**
 * A track's lines (`render` draws them; `null` is Not Playing). On a track change the old lines slide out
 * and the new ones in: to the left, or to the right after Previous. Spans throughout, so it can sit inside
 * a button; `className` styles the stack, `layerClassName` each track's lines.
 */
export function TrackSlide({
  transition: { shown, leaving, direction },
  className,
  layerClassName,
  render,
}: {
  transition: TrackTransition
  className: string
  layerClassName: string
  render(track: ShownTrack | null): ReactNode
}) {
  const layer = (state: 'entering' | 'leaving' | null) =>
    `track-slide-layer ${layerClassName}${state ? ` ${state}` : ''}`
  return (
    <span className={`track-slide ${direction} ${className}`}>
      {leaving && (
        <span key={`out:${leaving.track?.key ?? 'none'}`} className={layer('leaving')}>
          {render(leaving.track)}
        </span>
      )}
      <span key={shown?.key ?? 'none'} className={layer(leaving ? 'entering' : null)}>
        {render(shown)}
      </span>
    </span>
  )
}
