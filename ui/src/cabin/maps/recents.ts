import { create } from 'zustand'
import { createJSONStorage, persist } from 'zustand/middleware'
import type { Destination } from './places'

/** How many places Destinations remembers. */
const RECENTS = 6

interface Recents {
  /** The places last gone to or added as a stop, newest first. */
  places: Destination[]
  remember(place: Destination): void
}

/** The places the player last chose in Destinations (kept on this PC, like the rest of the cabin's state). */
export const useRecents = create<Recents>()(
  persist(
    (set) => ({
      places: [],
      remember: (place) =>
        set((state) => ({
          places: [place, ...state.places.filter((other) => other.node !== place.node)].slice(0, RECENTS),
        })),
    }),
    {
      name: 'westplay.recents',
      version: 1,
      storage: createJSONStorage(() => localStorage),
      partialize: (state) => ({ places: state.places }),
      // Stored by an earlier WestPlay (or edited): keep only what reads as a place.
      merge: (stored, current) => ({ ...current, places: validPlaces((stored as { places?: unknown })?.places) }),
    },
  ),
)

function validPlaces(places: unknown): Destination[] {
  if (!Array.isArray(places)) return []
  return places
    .filter(
      (place): place is Destination =>
        typeof place === 'object' &&
        place !== null &&
        typeof place.kind === 'string' &&
        typeof place.node === 'string' &&
        Array.isArray(place.at) &&
        place.at.length === 2,
    )
    .slice(0, RECENTS)
}
