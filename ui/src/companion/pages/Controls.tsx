import { useCallback, useState } from 'react'
import { useKeyName } from '../keys'
import { Group, KeyChooser, Note, Panel, Row, Slider, Switch } from '../room/parts'
import { n, useWords } from '../room/room'
import { GLOW, openAndClose, useScreenAt, useTour } from '../room/screen'
import { type Binding, DEFAULT_KEY, KEYBIND_IDS, type KeybindId, SENSITIVITY, type Settings } from '../useSettings'
import { KeyButton } from './KeyButton'
import type { PageProps } from './page'

/**
 * How the player uses the truck's screen: the key they hold (the onboarding's keycaps), how fast the pointer
 * goes, and whether the truck's own strip stays beside WestPlay; and every keybind (owner 2026-10-21): the
 * screen's key, any key, held or pressed once, and the music's volume and play/pause. On the room's screen
 * the pointer uses it, opening an app and coming back Home, again and again.
 */
export function Controls({ settings: state, cw }: PageProps) {
  const w = cw.controls
  const { settings, update, error } = state
  useScreenAt('home', GLOW.home, null)
  useTour([...openAndClose('music', GLOW.music), ...openAndClose('maps', GLOW.maps)])
  return (
    <Panel place="page" title={w.title} sub={w.sub}>
      <p className="ob-group-title ob-in" style={n(4)}>
        {settings?.interaction.mode === 'toggle' ? w.keyToggle : w.key}
      </p>
      <KeyChooser
        value={settings?.interaction.key ?? DEFAULT_KEY}
        disabled={!settings}
        place={4}
        choose={(key) => settings && update({ ...settings, interaction: { ...settings.interaction, key } })}
      />
      {settings && (
        <Group title={w.pointer} place={5}>
          <SpeedRow
            saved={settings.interaction.sensitivity}
            label={w.speed}
            detail={w.speedDetail}
            save={(sensitivity) => update({ ...settings, interaction: { ...settings.interaction, sensitivity } })}
          />
          <Row
            title={w.strip}
            detail={w.stripDetail}
            accessory={
              <Switch
                on={settings.screen.truckStrip === 'auto'}
                label={w.strip}
                change={(on) =>
                  update({ ...settings, screen: { ...settings.screen, truckStrip: on ? 'auto' : 'off' } })
                }
              />
            }
          />
        </Group>
      )}
      {settings && <Keybinds settings={settings} update={update} w={w} />}
      {error && <Note tone="bad">{error}</Note>}
    </Panel>
  )
}

/**
 * Every keybind: the screen's key (any key, not only the keycaps above), held or pressed once, and the music's
 * volume up and down and play/pause, each alone or with the screen's key. One listens for its key at a time;
 * a key taken already (the screen's, or another keybind's the same way) is refused, saying what it's for.
 */
function Keybinds({
  settings,
  update,
  w,
}: {
  settings: Settings
  update(settings: Settings): void
  w: PageProps['cw']['controls']
}) {
  const words = useWords()
  const [listening, setListening] = useState<KeybindId | 'screen' | null>(null)
  const [refused, setRefused] = useState<string | null>(null)
  const screenKey = settings.interaction.key
  const keyName = useKeyName(screenKey, words)
  const names: Record<KeybindId, string> = { volumeUp: w.volumeUp, volumeDown: w.volumeDown, playPause: w.playPause }
  const listen = useCallback(
    (which: KeybindId | 'screen') => (on: boolean) => {
      setListening(on ? which : null)
      if (on) setRefused(null)
    },
    [],
  )
  // What `binding` would clash with, if anything: the screen's key, or another keybind pressed the same way.
  const clash = (binding: Binding, except: KeybindId | 'screen'): string | null => {
    if (except !== 'screen' && binding.key === screenKey) return w.useScreen
    for (const id of KEYBIND_IDS) {
      if (id === except) continue
      const other = settings.keybinds[id]
      if (except === 'screen' ? other.key === binding.key : sameKeys(other, binding)) return names[id]
    }
    return null
  }
  const choose = (which: KeybindId | 'screen') => (binding: Binding) => {
    const taken = clash(binding, which)
    if (taken) {
      setRefused(w.inUse.replaceAll('{action}', taken))
      return
    }
    setRefused(null)
    if (which === 'screen') update({ ...settings, interaction: { ...settings.interaction, key: binding.key } })
    else update({ ...settings, keybinds: { ...settings.keybinds, [which]: binding } })
  }
  return (
    <>
      <Group title={w.keybinds} place={6}>
        <Row
          title={w.useScreen}
          accessory={
            <KeyButton
              binding={{ key: screenKey, withInteractionKey: false }}
              label={w.useScreen}
              listening={listening === 'screen'}
              listen={listen('screen')}
              choose={choose('screen')}
              words={words}
              pressKey={w.pressKey}
            />
          }
        />
        <Row
          title={w.toggle}
          detail={w.toggleDetail.replaceAll('{key}', keyName)}
          accessory={
            <Switch
              on={settings.interaction.mode === 'toggle'}
              label={w.toggle}
              change={(on) =>
                update({ ...settings, interaction: { ...settings.interaction, mode: on ? 'toggle' : 'hold' } })
              }
            />
          }
        />
        {KEYBIND_IDS.map((id) => (
          <Row
            key={id}
            title={names[id]}
            accessory={
              <KeyButton
                binding={settings.keybinds[id]}
                with={screenKey}
                label={names[id]}
                listening={listening === id}
                listen={listen(id)}
                choose={choose(id)}
                words={words}
                pressKey={w.pressKey}
              />
            }
          />
        ))}
      </Group>
      <Note tone={refused ? 'bad' : 'calm'}>{refused ?? w.keybindsNote.replaceAll('{key}', keyName)}</Note>
    </>
  )
}

/** Two bindings pressed the same way: the same key, alone or with the screen's key both. */
function sameKeys(a: Binding, b: Binding): boolean {
  return a.key === b.key && a.withInteractionKey === b.withInteractionKey
}

/** The pointer's speed: its slider follows the thumb, and it's saved once it's let go. A value saved
 * elsewhere (another window) resets it. */
function SpeedRow({
  saved,
  label,
  detail,
  save,
}: {
  saved: number
  label: string
  detail: string
  save(value: number): void
}) {
  const [dragging, setDragging] = useState(saved)
  const [lastSaved, setLastSaved] = useState(saved)
  if (saved !== lastSaved) {
    setLastSaved(saved)
    setDragging(saved)
  }
  return (
    <div className="ob-row cp-slider-row">
      <span className="ob-row-text">
        <b>{label}</b>
        <span>{detail}</span>
      </span>
      <span className="ob-row-value">{`${dragging.toFixed(1)}×`}</span>
      <Slider
        value={dragging}
        min={SENSITIVITY.min}
        max={SENSITIVITY.max}
        step={SENSITIVITY.step}
        label={label}
        change={setDragging}
        commit={save}
      />
    </div>
  )
}
