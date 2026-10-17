import { useEffect, useState } from 'react'
import { Button, Group, Note, Row, Switch, Tick } from '../room/parts'
import { banner, explain, useGameRunning, useWords } from '../room/room'
import { useScreen } from '../room/screen'
import { leavePc, type UninstallReport, uninstallFromGame } from '../setup'
import type { CompanionWords } from './words'

/** How long each step's tick takes to follow the last, once they're done. */
const TICK_MS = 380
/** The goodbye on the screen, before it goes dark. */
const GOODBYE_MS = 1700
/** The screen dark and the room fading, before WestPlay quits. */
const FAREWELL_MS = 1900

const STEPS = ['profiles', 'mod', 'plugin', 'data'] as const

/**
 * Uninstalling WestPlay, in a sheet over the room: what goes (and a choice to keep settings and maps), then
 * each step ticked off as it's done, the screen saying so. Done, the screen says goodbye and goes dark, the
 * room fades, and WestPlay leaves the PC and quits (`leavePc`). The game has to be closed; the plugin may
 * need Windows' permission, as when it was installed.
 */
export function UninstallSheet({
  cw,
  close,
  farewell,
}: {
  cw: CompanionWords
  close(): void
  /** The room fades: WestPlay is leaving. */
  farewell(): void
}) {
  const w = cw.uninstall
  const screen = useScreen()
  const words = useWords()
  const running = useGameRunning()
  const [keep, setKeep] = useState(false)
  const [working, setWorking] = useState(false)
  const [report, setReport] = useState<UninstallReport | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [ticked, setTicked] = useState(0)
  const done = report?.done ?? false

  // Done: the steps tick off in turn, the screen says goodbye and goes dark, and WestPlay leaves.
  useEffect(() => {
    if (!done) return
    const timers = STEPS.map((_, at) => window.setTimeout(() => setTicked(at + 1), (at + 1) * TICK_MS))
    const end = STEPS.length * TICK_MS
    timers.push(window.setTimeout(() => screen.banner(banner(w.banners.done)), end))
    timers.push(
      window.setTimeout(() => {
        screen.banner(null)
        screen.send({ type: 'power', on: false })
        farewell()
      }, end + GOODBYE_MS),
    )
    timers.push(
      window.setTimeout(() => leavePc(keep).catch((e: unknown) => setError(String(e))), end + GOODBYE_MS + FAREWELL_MS),
    )
    return () => timers.forEach(window.clearTimeout)
    // Once, as it's done.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [done])

  const uninstall = async (administrator: boolean) => {
    setWorking(true)
    setError(null)
    screen.banner(banner(w.banners.working, true))
    try {
      const result = await uninstallFromGame(administrator)
      setReport(result)
      if (!result.done) screen.banner(null)
    } catch (e) {
      setError(String(e) === 'gameRunning' ? null : explain(words, e))
      screen.banner(null)
    } finally {
      setWorking(false)
    }
  }

  const plugin = report?.plugin
  const asAdministrator = plugin?.state === 'needsAdministrator' || plugin?.state === 'declined'
  const busy = working || done
  return (
    <div className="cp-veil" onClick={busy ? undefined : close}>
      <section className="cp-sheet" role="dialog" aria-label={w.title} onClick={(event) => event.stopPropagation()}>
        <h2>{done && ticked === STEPS.length ? w.doneTitle : w.title}</h2>
        <p>{done && ticked === STEPS.length ? w.doneSub : w.text}</p>
        {busy ? (
          <ol className="ob-stages cp-sheet-steps">
            {STEPS.map((step, at) => (
              <li key={step} data-now={(!done && at === 0) || (done && at === ticked) || undefined}>
                {at < ticked ? <Tick done /> : <i className="ob-working" />}
                <span>{w.steps[step]}</span>
              </li>
            ))}
          </ol>
        ) : (
          <Group place={0}>
            <Row
              title={w.keep}
              detail={w.keepDetail}
              accessory={<Switch on={keep} label={w.keep} change={setKeep} />}
            />
          </Group>
        )}
        {running && !busy && (
          <Note>
            <b>{w.closeGame.title}</b> {w.closeGame.text}
          </Note>
        )}
        {plugin?.state === 'needsAdministrator' && (
          <Note tone="calm">
            <b>{words.install.needsAdministrator.title}</b> {words.install.needsAdministrator.text}
          </Note>
        )}
        {plugin?.state === 'declined' && (
          <Note>
            <b>{words.install.declined.title}</b> {words.install.declined.text}
          </Note>
        )}
        {plugin?.state === 'inUse' && (
          <Note>
            <b>{w.closeGame.title}</b> {w.closeGame.text}
          </Note>
        )}
        {plugin?.state === 'failed' && <Note tone="bad">{explain(words, plugin.error)}</Note>}
        {report?.modError && <Note tone="bad">{report.modError}</Note>}
        {report && report.profilesFailed > 0 && (
          <Note>{w.profilesFailed.replace('{n}', String(report.profilesFailed))}</Note>
        )}
        {error && <Note tone="bad">{error}</Note>}
        {!busy && (
          <div className="cp-sheet-actions">
            <Button glass onClick={close}>
              {w.cancel}
            </Button>
            <Button danger onClick={() => uninstall(asAdministrator)} disabled={running !== false}>
              {asAdministrator ? w.asAdministrator : w.confirm}
            </Button>
          </div>
        )}
      </section>
    </div>
  )
}
