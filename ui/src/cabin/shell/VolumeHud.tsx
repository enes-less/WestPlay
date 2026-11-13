import { SunGlyph } from '../apps/settings/glyphs'
import { VolumeGlyph } from '../apps/spotify/glyphs'
import { useVolumeHud } from './volumeShown'
import './volumeHud.css'

/**
 * The volume, as CarPlay shows it when the steering wheel's buttons change it: a glass pill at the top of the
 * screen with a speaker and the level, for a moment after each change (apps/keybinds.ts, volumeShown.ts). The
 * screen's brightness the same, with a sun (owner 2026-11-13: its keybinds shown as the volume's).
 */
export function VolumeHud() {
  const { kind, level, shown } = useVolumeHud()
  return (
    <div className={shown ? 'volume-hud shown' : 'volume-hud'} aria-hidden={!shown}>
      <div className="volume-hud-pill">
        {kind === 'brightness' ? <SunGlyph large /> : <VolumeGlyph volume={level} />}
        <div className="volume-hud-bar">
          <i style={{ width: `${Math.round(level * 100)}%` }} />
        </div>
      </div>
    </div>
  )
}
