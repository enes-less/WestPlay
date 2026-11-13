import { Button } from '../room/parts'
import type { CompanionWords } from './words'

/**
 * The game started while a map was being prepared, which stopped it (owner 2026-11-12: players are told to wait
 * for their map before they play): what happened, that it starts over once the game's closed, and to wait. Only
 * its OK closes it (`useGameStopped` puts it up).
 */
export function GameStoppedSheet({ cw, close }: { cw: CompanionWords; close(): void }) {
  const w = cw.map.gameStopped
  return (
    <div className="cp-veil">
      <section className="cp-sheet" role="alertdialog" aria-label={w.title}>
        <h2>{w.title}</h2>
        <p>{w.text}</p>
        <div className="cp-sheet-actions">
          <Button onClick={close}>{w.ok}</Button>
        </div>
      </section>
    </div>
  )
}
