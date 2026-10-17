import { useContext, useState } from 'react'
import { hvtMapMessage } from '../../cabin/maps/blockedMap'
import { Checking, Group, MapProgress, Note, Panel, ProfileRow, Row, Tick } from '../room/parts'
import { banner, useGlidingProgress, WordsContext } from '../room/room'
import { GLOW, ON_SCREEN, useScreenAt, useTour } from '../room/screen'
import { percent } from '../onboarding/words'
import { memoryNote, type PageProps, useGameCheck } from './page'

/**
 * The map in the cab's Maps app: made from the player's own game, for the profile they play. Its state (built
 * when, or being built now, as in the onboarding), and each profile's map with its map mods. On the room's
 * screen, Maps with the player's map, zooming out and back in.
 */
export function MapPage(props: PageProps) {
  const { status, cw, check } = props
  const { words, code } = useContext(WordsContext)
  const w = words.map
  const map = status?.map ?? { state: 'unknown' as const }
  const building = map.state === 'building' ? map : null
  const shown = useGlidingProgress(building?.done ?? null, building?.stage ?? null)
  // Seen building on this visit: once it's ready, its stages stay up, all ticked.
  const [watched, setWatched] = useState(false)
  if (building && !watched) setWatched(true)
  const error = useGameCheck(props)
  const [hvtTitle, hvtText] = [
    ...new Intl.Segmenter(words.locale, { granularity: 'sentence' }).segment(hvtMapMessage(code)),
  ].map((part) => part.segment.trim())

  useScreenAt(
    'maps',
    GLOW.maps,
    map.state === 'building'
      ? { title: w.banners.building, detail: `${w.stages[map.stage].now} · ${percent(words, shown)}`, progress: shown }
      : map.state === 'waitingForGame'
        ? banner(w.banners.waiting)
        : map.state === 'waitingForMemory'
          ? banner(cw.map.memory.banner)
          : map.state === 'notAllowed'
            ? { title: hvtTitle, detail: hvtText }
            : map.state === 'failed'
              ? banner(w.banners.failed)
              : null,
  )
  useTour(
    map.state === 'ready'
      ? [
          { tap: ON_SCREEN.zoomOut, then: 700 },
          { tap: ON_SCREEN.zoomOut, then: 3200 },
          { tap: ON_SCREEN.zoomIn, then: 700 },
          { tap: ON_SCREEN.zoomIn, then: 4200 },
        ]
      : null,
    1800,
  )

  const title =
    map.state === 'building'
      ? cw.map.titles.building
      : map.state === 'waitingForGame'
        ? cw.map.titles.waiting
        : map.state === 'waitingForMemory'
          ? cw.map.memory.title
          : map.state === 'failed' || map.state === 'notAllowed'
            ? cw.map.titles.failed
            : map.state === 'noGame'
              ? cw.map.titles.noGame
              : cw.map.titles.ready
  const built = status?.mapBuilt
  return (
    <Panel place="page" title={title} sub={cw.map.sub}>
      {(building || (watched && map.state === 'ready')) && (
        <MapProgress shown={shown} stage={building?.stage ?? null} finished={map.state === 'ready'} />
      )}
      {map.state === 'ready' && !watched && (
        <Group place={4}>
          <Row
            title={cw.map.inCab}
            detail={
              built
                ? cw.map.built.replace(
                    '{date}',
                    new Intl.DateTimeFormat(words.locale, { dateStyle: 'medium' }).format(new Date(built * 1000)),
                  )
                : cw.map.notBuilt
            }
            accessory={<Tick done />}
          />
        </Group>
      )}
      {map.state === 'waitingForGame' && (
        <Note>
          <b>{w.waiting.title}</b> {w.waiting.text}
        </Note>
      )}
      {map.state === 'waitingForMemory' && (
        <Note>
          <b>{cw.map.memory.note.title}</b> {memoryNote(cw, words.locale, map.needed, map.free)}
        </Note>
      )}
      {map.state === 'failed' && <Note tone="bad">{map.error}</Note>}
      {map.state === 'notAllowed' && <Note tone="bad">{hvtMapMessage(code)}</Note>}
      <Group title={cw.map.profiles} place={5}>
        {check ? (
          check.profiles.length ? (
            check.profiles.map((profile) => <ProfileRow key={profile.name} profile={profile} />)
          ) : (
            <Row title={words.check.noProfiles} detail={words.check.noProfilesDetail} />
          )
        ) : (
          <Row title={error ?? cw.map.checking} accessory={error ? undefined : <Checking />} />
        )}
      </Group>
    </Panel>
  )
}
