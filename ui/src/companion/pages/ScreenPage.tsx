import { setWestplayOpen } from '../useHostStatus'
import { Group, Note, Panel, Row, Status, Switch, Tick } from '../room/parts'
import { banner } from '../room/room'
import { GLOW, useScreenAt } from '../room/screen'
import { type PageProps, screenState } from './page'

/**
 * The truck's screen: where WestPlay is now (its headline), WestPlay on it or the truck's own, and the
 * connection from WestPlay to the game to the screen. On the room's screen, Home, and WestPlay saying the
 * same in a banner.
 */
export function ScreenPage({ status, settings, cw }: PageProps) {
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
        : state === 'unsupported'
          ? banner(w.banners.unsupported)
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
          value={
            state === 'unsupported'
              ? w.screenUnsupported
              : status?.screenDrawing
                ? w.screenLive.replace('{fps}', String(fps))
                : w.screenNotInView
          }
          accessory={
            <Status
              tone={
                state === 'unsupported'
                  ? 'idle'
                  : status?.screenDrawing
                    ? 'good'
                    : status?.gameConnected
                      ? 'busy'
                      : 'idle'
              }
            />
          }
        />
      </Group>
      {status?.gameElevated && (
        <Note tone="bad">
          <b>{w.elevated.title}</b> {w.elevated.text}
        </Note>
      )}
      <GpuChooser status={status} settings={settings} cw={cw} />
      {status?.error && <Note tone="bad">{status.error}</Note>}
    </Panel>
  )
}

/**
 * The graphics card WestPlay's screen is drawn on: WestPlay's pick (the game's), or one the player chooses
 * when that one stutters, as iOS lists choices (a tick on the chosen, as the onboarding's game folders). The
 * one drawing it now says so; a choice starts WestPlay's screen again on that card. Only with more than one
 * card to choose from.
 */
function GpuChooser({ status, settings: state, cw }: Pick<PageProps, 'status' | 'settings' | 'cw'>) {
  const w = cw.screen
  const { settings, update } = state
  const adapters = status?.adapters ?? []
  if (!settings || adapters.length < 2) return null
  const chosen = adapters.some((adapter) => adapter.id === settings.graphics.gpu) ? settings.graphics.gpu : null
  const choose = (gpu: string | null) => {
    if (gpu !== settings.graphics.gpu) void update({ ...settings, graphics: { gpu } })
  }
  return (
    <>
      <Group title={cw.about.gpu} place={6}>
        <Row
          title={w.gpuAuto}
          detail={w.gpuAutoDetail}
          accessory={chosen === null ? <Tick key="auto" /> : undefined}
          onClick={() => choose(null)}
        />
        {adapters.map((adapter) => (
          <Row
            key={adapter.id}
            title={adapter.name}
            detail={adapter.name === status?.cabAdapter ? w.gpuInUse : undefined}
            accessory={chosen === adapter.id ? <Tick key={adapter.id} /> : undefined}
            onClick={() => choose(adapter.id)}
          />
        ))}
      </Group>
      <Note tone="calm">{w.gpuNote}</Note>
    </>
  )
}
