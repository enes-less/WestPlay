import { type ClockStyle, formatGameTime, formatTripDistance, formatTripTime, HOURS_24 } from '../../shared/format'
import { cabClock, type JobInfo, type Telemetry } from '../../shared/host'
import { labelLines } from '../maps/labels'
import type { MapLabel } from '../maps/mapData'
import { metresBetween, toLngLat } from '../maps/projection'
import type { Point } from '../maps/route'
import type { Strings } from '../state/strings'

/**
 * Where the truck is headed, for the Dashboard's destination widget: the job (its pickup, then its
 * delivery, as the game reports it: companies and cities in the game's language), or else where the game's
 * route ends (a point the player put on the map: named after the nearest city), or nothing.
 */
export interface DestinationView {
  /** What it is: "Delivery", "Pickup", "Destination". */
  kicker: string
  title: string
  /** The city under a company's name. */
  subtitle: string | null
  /** The deadline and distance, the cargo, or the arrival and distance. */
  detail: string
  /** Past the delivery deadline. */
  late: boolean
}

/** A day of the week (day 0 is a Monday) and the time (by the clock's `style`), or only the time if it's today. */
export function formatDeadline(
  deadline: number,
  now: number,
  days: readonly string[],
  style: ClockStyle = HOURS_24,
): string {
  const time = formatGameTime(deadline, style)
  return Math.floor(deadline / 1440) === Math.floor(now / 1440)
    ? time
    : `${days[Math.floor(deadline / 1440) % 7]} ${time}`
}

/** The city nearest `at` (game metres), by its name as the map shows it; `null` without any. */
export function nearestCity(at: Point, labels: readonly MapLabel[]): string | null {
  const target = toLngLat(at[0], at[1])
  let best: { label: MapLabel; distance: number } | null = null
  for (const label of labels) {
    if (label.kind !== 'city') continue
    const distance = metresBetween(target, label.at)
    if (!best || distance < best.distance) best = { label, distance }
  }
  return best && labelLines(best.label.name, best.label.localized).first
}

export function destinationView(
  job: JobInfo | null,
  routeEnd: Point | null,
  telemetry: Telemetry | null,
  labels: readonly MapLabel[],
  text: Strings,
  clockShift: number,
  clock: ClockStyle = HOURS_24,
  mph = false,
): DestinationView | null {
  // Late or not by the player's own time, as the job's; shown by the cab's clock (a Convoy session's, in
  // the time zone where the truck is: `cabClock`).
  const now = telemetry?.gameTime ?? 0
  const cab = telemetry ? cabClock(telemetry, clockShift) : 0
  const routeLeft = telemetry && telemetry.navigationDistance > 0 ? telemetry.navigationDistance : null
  if (job) {
    if (!job.cargoLoaded) {
      const tonnes = (job.cargoMass / 1000).toFixed(1).replace('.', text.decimal)
      return {
        kicker: text.pickup,
        title: job.sourceCompany || job.sourceCity,
        subtitle: job.sourceCompany ? job.sourceCity : null,
        detail: job.cargo ? `${job.cargo} · ${tonnes} t` : `${tonnes} t`,
        late: false,
      }
    }
    const distance =
      routeLeft !== null
        ? formatTripDistance(routeLeft, text.decimal, mph)
        : mph
          ? `${Math.round((job.plannedDistanceKm * 1000) / 1609.344)} mi`
          : `${job.plannedDistanceKm} km`
    const late = job.deliveryTime > 0 && now > job.deliveryTime
    const deadline =
      job.deliveryTime > 0
        ? late
          ? text.late
          : text.arriveBy(formatDeadline(job.deliveryTime + (cab - now), cab, text.days, clock))
        : null
    return {
      kicker: text.delivery,
      title: job.destinationCompany || job.destinationCity,
      subtitle: job.destinationCompany ? job.destinationCity : null,
      detail: deadline ? `${deadline} · ${distance}` : distance,
      late,
    }
  }
  if (routeEnd && telemetry && routeLeft !== null) {
    const minutes = telemetry.navigationTime / 60
    return {
      kicker: text.destination,
      title: nearestCity(routeEnd, labels) ?? text.mapPoint,
      subtitle: null,
      detail: `${formatGameTime(Math.round(cab + minutes), clock)} ${text.arrival} · ${formatTripTime(minutes, text)}`,
      late: false,
    }
  }
  return null
}
