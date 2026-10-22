import { create } from 'zustand'

/** How long the volume stays shown after the last change, ms. */
const SHOWN_MS = 1500

/** The volume a keybind last set (0 to 1), and whether it's shown (VolumeHud.tsx). */
export const useVolumeHud = create<{ volume: number; shown: boolean }>()(() => ({ volume: 0, shown: false }))

let hiding: ReturnType<typeof setTimeout> | undefined

/** Shows `volume` (0 to 1) for a moment: a keybind changed it. */
export function showVolume(volume: number): void {
  clearTimeout(hiding)
  useVolumeHud.setState({ volume, shown: true })
  hiding = setTimeout(() => useVolumeHud.setState({ shown: false }), SHOWN_MS)
}
