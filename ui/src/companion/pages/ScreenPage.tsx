import { setWestplayOpen } from '../useHostStatus'
import { Group, Note, Panel, Row, Status, Switch } from '../room/parts'
import { banner } from '../room/room'
import { GLOW, useScreenAt } from '../room/screen'
import { type PageProps, screenState } from './page'

/**
 * The truck's screen: where WestPlay is now (its headline), WestPlay on it or the truck's own, and the
 * connection from WestPlay to the game to the screen. On the room's screen, Home, and WestPlay saying the
 * same in a banner.
 */
export function ScreenPage({ status, cw }: PageProps) {
  const w = cw.screen
  const state = screenState(status)
  // Known once the cabin page has said (it owns the state): the switch waits for it.
  const open = status?.westplayOpen ?? null
  useScreenAt(
    state === 'original' ? 'dashboard' : 'home',
    GLOW.home,
    state === 'live'
      ? banner(w.banners.live)
      : state === 'original'
        ? banner(w.banners.original)
        : state === 'notInCab'
          ? banner(w.banners.notInCab)
          : state === 'noGame'
            ? banner(w.banners.noGame, true)
            : null,
  )
  const fps = status?.framesPerSecond ?? 0
  return (
    <Panel place="page" title={w.titles[state]} sub={w.subs[state]}>
      <Group place={4}>
        <Row
          title={w.show}
          detail={w.showDetail}
          accessory={
            <Switch
              on={open ?? false}
              disabled={open === null || !status?.screenDrawing}
              label={w.show}
              change={(on) => {
                setWestplayOpen(on).catch((error: unknown) => console.error('WestPlay could not be switched', error))
              }}
            />
          }
        />
      </Group>
      <Group title={w.connection} place={5}>
        <Row
          title={w.engine}
          value={status?.error ? cw.values.failed : status ? w.engineRunning : w.engineStarting}
          accessory={<Status tone={status?.error ? 'bad' : status ? 'good' : 'busy'} />}
        />
        <Row
          title={w.game}
          value={status?.gameConnected ? w.gameConnected : w.gameNotRunning}
          accessory={<Status tone={status?.gameConnected ? 'good' : 'idle'} />}
        />
        <Row
          title={w.truckScreen}
          value={status?.screenDrawing ? w.screenLive.replace('{fps}', String(fps)) : w.screenNotInView}
          accessory={<Status tone={status?.screenDrawing ? 'good' : status?.gameConnected ? 'busy' : 'idle'} />}
        />
      </Group>
      {status?.error && <Note tone="bad">{status.error}</Note>}
    </Panel>
  )
}
