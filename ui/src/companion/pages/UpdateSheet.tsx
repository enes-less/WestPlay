import { useEffect, useRef, useState } from 'react'
import { Button, Note, Tick } from '../room/parts'
import { banner, explain, useGameRunning, useWords } from '../room/room'
import { useScreen } from '../room/screen'
import { type InstallReport, installIntoGame, type SetupState } from '../setup'
import { installUpdate, noteLines, type Offer, onUpdateProgress, type Progress } from '../update'
import type { CompanionWords } from './words'

/**
 * An update to WestPlay, offered in a sheet over the room: its version, what's new (the changelog's notes,
 * in English as they're written), and Later or Update. Updating downloads it (its progress shown), then
 * Windows' installer puts it in and starts WestPlay again: the app quits for it. The game has to be closed:
 * the plugin it loaded is this WestPlay's.
 */
export function UpdateSheet({ cw, offer, close }: { cw: CompanionWords; offer: Offer; close(): void }) {
  const w = cw.update
  const words = useWords()
  const screen = useScreen()
  const running = useGameRunning()
  const [working, setWorking] = useState(false)
  const [progress, setProgress] = useState<Progress | null>(null)
  const [error, setError] = useState<string | null>(null)
  const lines = noteLines(offer.notes)
  const downloaded = progress?.total ? progress.done >= progress.total : false

  useEffect(() => (working ? onUpdateProgress(setProgress) : undefined), [working])

  const update = async () => {
    setWorking(true)
    setError(null)
    setProgress(null)
    screen.banner(banner(w.banners.updating, true))
    try {
      // Comes back only if it couldn't be installed: otherwise WestPlay quits for the installer.
      await installUpdate()
    } catch (e) {
      setError(String(e) === 'gameRunning' ? null : `${w.failed} ${explain(words, e)}`)
      setWorking(false)
      screen.banner(null)
    }
  }

  return (
    <div className="cp-veil" onClick={working ? undefined : close}>
      <section
        className="cp-sheet"
        role="dialog"
        aria-label={w.title.replace('{version}', offer.version)}
        onClick={(event) => event.stopPropagation()}
      >
        <h2>{w.title.replace('{version}', offer.version)}</h2>
        <p>{w.sub}</p>
        {working ? (
          <ol className="ob-stages cp-sheet-steps">
            <li data-now={!downloaded || undefined}>
              {downloaded ? <Tick done /> : <i className="ob-working" />}
              <span>
                {w.downloading}
                {progress?.total ? ` ${percent(progress, words.locale)}` : ''}
              </span>
            </li>
            <li data-now={downloaded || undefined}>
              <i className="ob-working" />
              <span>{w.installing}</span>
            </li>
          </ol>
        ) : (
          lines.length > 0 && (
            <div className="cp-sheet-notes">
              <b>{w.whatsNew}</b>
              <ul>
                {lines.map((line, at) => (
                  <li key={at}>{line}</li>
                ))}
              </ul>
            </div>
          )
        )}
        {running && !working && (
          <Note>
            <b>{w.closeGame.title}</b> {w.closeGame.text}
          </Note>
        )}
        {error && <Note tone="bad">{error}</Note>}
        {!working && (
          <div className="cp-sheet-actions">
            <Button glass onClick={close}>
              {w.later}
            </Button>
            <Button onClick={update} disabled={running !== false}>
              {w.install}
            </Button>
          </div>
        )}
      </section>
    </div>
  )
}

function percent(progress: Progress, locale: string): string {
  const share = Math.min(1, progress.done / (progress.total ?? 1))
  return new Intl.NumberFormat(locale, { style: 'percent' }).format(share)
}

/**
 * After an update, the new WestPlay's plugin and mod go into the game (setup's `updated`), once it's closed:
 * on their own when they can, the screen saying so. A game folder that needs administrator rights (or the
 * player said no to Windows) asks first, in a sheet, until it's done or put off (the Setup page's reinstall
 * then).
 */
export function FinishUpdate({ cw, setup }: { cw: CompanionWords; setup: SetupState }) {
  const w = cw.update.finish
  const words = useWords()
  const screen = useScreen()
  const running = useGameRunning()
  const [report, setReport] = useState<InstallReport | null>(null)
  const [working, setWorking] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [later, setLater] = useState(false)
  const tried = useRef(false)
  const game = setup.updated ? setup.game : null
  const done = report?.done ?? false

  const finish = async (administrator: boolean) => {
    if (!game) return
    setWorking(true)
    setError(null)
    try {
      const result = await installIntoGame(game.dir, administrator)
      setReport(result)
      if (result.done) {
        screen.reboot()
        screen.banner(banner(cw.update.banners.updated))
      }
    } catch (e) {
      setError(explain(words, e))
    } finally {
      setWorking(false)
    }
  }

  // On its own, once, as soon as the game is closed (again if it was started while it went in).
  useEffect(() => {
    if (!game || running !== false || done) return
    if (tried.current && report?.plugin.state !== 'inUse') return
    tried.current = true
    void finish(false)
    // Each time the game closes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [game, running])

  const plugin = report?.plugin
  const administrator = plugin?.state === 'needsAdministrator' || plugin?.state === 'declined'
  const asks = administrator || plugin?.state === 'failed'
  if (!game || done || later || !(asks || report?.modError || error)) return null
  return (
    <div className="cp-veil" onClick={working ? undefined : () => setLater(true)}>
      <section className="cp-sheet" role="dialog" aria-label={w.title} onClick={(event) => event.stopPropagation()}>
        <h2>{w.title}</h2>
        <p>{w.text}</p>
        {plugin?.state === 'declined' && (
          <Note>
            <b>{words.install.declined.title}</b> {words.install.declined.text}
          </Note>
        )}
        {plugin?.state === 'failed' && (
          <Note tone="bad">
            <b>{words.install.pluginFailed}</b> {explain(words, plugin.error)}
          </Note>
        )}
        {report?.modError && (
          <Note tone="bad">
            <b>{words.install.modFailed}</b> {explain(words, report.modError)}
          </Note>
        )}
        {error && <Note tone="bad">{error}</Note>}
        {running && (
          <Note>
            <b>{cw.update.closeGame.title}</b> {cw.update.closeGame.text}
          </Note>
        )}
        <div className="cp-sheet-actions">
          <Button glass onClick={() => setLater(true)} disabled={working}>
            {cw.update.later}
          </Button>
          <Button onClick={() => finish(administrator)} busy={working} disabled={running !== false}>
            {w.button}
          </Button>
        </div>
      </section>
    </div>
  )
}
