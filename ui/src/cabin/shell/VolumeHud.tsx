import { VolumeGlyph } from '../apps/spotify/glyphs'
import { useVolumeHud } from './volumeShown'
import './volumeHud.css'

/**
 * The volume, as CarPlay shows it when the steering wheel's buttons change it: a glass pill at the top of the
 * screen with a speaker and the level, for a moment after each change (apps/keybinds.ts, volumeShown.ts).
 */
export function VolumeHud() {
  const { volume, shown } = useVolumeHud()
  return (
    <div className={shown ? 'volume-hud shown' : 'volume-hud'} aria-hidden={!shown}>
      <div className="volume-hud-pill">
        <VolumeGlyph volume={volume} />
        <div className="volume-hud-bar">
          <i style={{ width: `${Math.round(volume * 100)}%` }} />
        </div>
      </div>
    </div>
  )
}
