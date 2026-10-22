import { roundShown } from './mapRound'
import { useGlidingProgress } from './room/room'
import type { HostStatus, MapStatus } from './useHostStatus'

/** A map being made, as the host says. */
export type Building = Extract<MapStatus, { state: 'building' }>

/**
 * The maps being prepared, as the Map page's bar and the sidebar's value show them, worked out once for both
 * (they showed different numbers when each worked out its own, 2026-10-21): the map being made now (the
 * played profile's, else another's, as the maps are made one after another), how far it is (gliding between
 * the builder's words, never far ahead of them), and how far the round is (mapRound.ts: each map an equal
 * share).
 */
export interface MapProgress {
  building: Building | null
  /** Which map is being made: the played profile's (`played`), another's folder, or none. */
  buildingWhich: string | null
  /** A map is being made or waits its turn. */
  pending: boolean
  /** The played profile's map is ready and none is being made or waits. */
  finished: boolean
  /** The map being made's own progress, as shown (0 to 1). */
  making: number
  /** The round's progress, as shown: the bar's number and the sidebar's. */
  shown: number
}

export function useMapProgress(status: HostStatus | null): MapProgress {
  const map = status?.map ?? { state: 'unknown' as const }
  const others = status?.profileMaps ?? []
  const another = others.find((each) => each.status.state === 'building')
  const building = map.state === 'building' ? map : another?.status.state === 'building' ? another.status : null
  const buildingWhich = map.state === 'building' ? 'played' : (another?.dir ?? null)
  const pending = others.some((each) => each.status.state === 'building' || each.status.state === 'queued')
  const making = useGlidingProgress(building?.done ?? null, building?.stage ?? null, buildingWhich)
  return {
    building,
    buildingWhich,
    pending,
    finished: map.state === 'ready' && !pending,
    making,
    shown: roundShown(status, making) ?? making,
  }
}
