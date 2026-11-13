import { create } from 'zustand'

/** How long the level stays shown after the last change, ms. */
const SHOWN_MS = 1500

/** What a keybind last changed: the volume or the screen's brightness, its level (0 to 1), and whether it's shown
 * (VolumeHud.tsx). */
export const useVolumeHud = create<{ kind: 'volume' | 'brightness'; level: number; shown: boolean }>()(() => ({
  kind: 'volume',
  level: 0,
  shown: false,
}))

let hiding: ReturnType<typeof setTimeout> | undefined

function show(kind: 'volume' | 'brightness', level: number): void {
  clearTimeout(hiding)
  useVolumeHud.setState({ kind, level, shown: true })
  hiding = setTimeout(() => useVolumeHud.setState({ shown: false }), SHOWN_MS)
}

/** Shows `volume` (0 to 1) for a moment: a keybind changed it. */
export function showVolume(volume: number): void {
  show('volume', volume)
}

/** Shows the screen's brightness `level` (0 to 1) for a moment, as the volume is: a keybind changed it. */
export function showBrightness(level: number): void {
  show('brightness', level)
}
