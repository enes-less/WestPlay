import { useText } from '../state/text'
import { recentreLiveMap, useMapBrowsing, zoomLiveMap } from './liveMap'

/**
 * The Maps app's buttons over the map, like CarPlay's: zoom in and out, and, once the map has been moved
 * away from the truck, recentre. (The wheel, dragging and right-dragging do the same with the mouse.)
 */
export function MapControls() {
  const browsing = useMapBrowsing((state) => state.browsing)
  const text = useText()
  return (
    <div className="map-controls">
      <button
        type="button"
        className={browsing ? 'map-control map-recentre glass shown' : 'map-control map-recentre glass'}
        aria-label={text.recentre}
        aria-hidden={!browsing}
        tabIndex={browsing ? 0 : -1}
        onClick={recentreLiveMap}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M20.5 3.5 3.8 10.6c-.8.4-.7 1.6.2 1.8l6.6 1.1 1.1 6.6c.2.9 1.4 1 1.8.2Z" fill="currentColor" />
        </svg>
      </button>
      <div className="map-zoom glass">
        <button
          type="button"
          className="map-control"
          data-control="zoom-in"
          aria-label={text.zoomIn}
          onClick={() => zoomLiveMap(1)}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M12 5v14M5 12h14" />
          </svg>
        </button>
        <span className="map-zoom-divider" aria-hidden />
        <button
          type="button"
          className="map-control"
          data-control="zoom-out"
          aria-label={text.zoomOut}
          onClick={() => zoomLiveMap(-1)}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M5 12h14" />
          </svg>
        </button>
      </div>
    </div>
  )
}
