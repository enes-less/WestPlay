import { Button, Group, Row, Tick } from '../room/parts'
import { useWords } from '../room/room'
import { fill } from '../onboarding/words'
import { gameLanguages, nameIn, ownName } from '../languages'
import type { SettingsState } from '../useSettings'
import type { CompanionWords } from './words'

/**
 * The language WestPlay speaks, in the cab and here (owner 2026-11-03: the player may choose one over the
 * game's), in a sheet over the room as iOS lists languages: Automatic (the game's), then each of the game's by
 * its own name, with its name in the language now shown under it; a tick on the chosen. Choosing one closes the
 * sheet, and the companion and the cab change to it at once.
 */
export function LanguageSheet({
  cw,
  settings: state,
  game,
  close,
}: {
  cw: CompanionWords
  settings: SettingsState
  /** The language Automatic is: the game's (or Windows', for a game never started). */
  game: string
  close(): void
}) {
  const words = useWords()
  const w = cw.setup
  const { settings, update } = state
  if (!settings) return null
  const choose = (language: string | null) => {
    if (language !== settings.language) void update({ ...settings, language })
    close()
  }
  return (
    <div className="cp-veil" onClick={close}>
      <section className="cp-sheet" role="dialog" aria-label={w.language} onClick={(event) => event.stopPropagation()}>
        <h2>{w.language}</h2>
        <p>{w.languageNote}</p>
        <div className="cp-sheet-list">
          <Group place={0}>
            <Row
              title={w.languageAutomatic}
              detail={fill(w.languageAutomaticDetail, { name: ownName(game) })}
              accessory={settings.language === null ? <Tick key="auto" /> : undefined}
              onClick={() => choose(null)}
            />
            {gameLanguages().map((code) => (
              <Row
                key={code}
                title={ownName(code)}
                detail={nameIn(code, words.locale) ?? undefined}
                accessory={settings.language === code ? <Tick key={code} /> : undefined}
                onClick={() => choose(code)}
              />
            ))}
          </Group>
        </div>
        <div className="cp-sheet-actions">
          <Button glass onClick={close}>
            {words.launch.done}
          </Button>
        </div>
      </section>
    </div>
  )
}
