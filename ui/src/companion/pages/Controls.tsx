import { useState } from 'react'
import { Group, KeyChooser, Note, Panel, Row, Slider, Switch } from '../room/parts'
import { n } from '../room/room'
import { GLOW, openAndClose, useScreenAt, useTour } from '../room/screen'
import { SENSITIVITY } from '../useSettings'
import type { PageProps } from './page'

/**
 * How the player uses the truck's screen: the key they hold (the onboarding's keycaps), how fast the pointer
 * goes, and whether the truck's own strip stays beside WestPlay. On the room's screen the pointer uses it,
 * opening an app and coming back Home, again and again.
 */
export function Controls({ settings: state, cw }: PageProps) {
  const w = cw.controls
  const { settings, update, error } = state
  useScreenAt('home', GLOW.home, null)
  useTour([...openAndClose('music', GLOW.music), ...openAndClose('maps', GLOW.maps)])
  return (
    <Panel place="page" title={w.title} sub={w.sub}>
      <p className="ob-group-title ob-in" style={n(4)}>
        {w.key}
      </p>
      <KeyChooser
        value={settings?.interaction.key ?? 'leftAlt'}
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
      {error && <Note tone="bad">{error}</Note>}
    </Panel>
  )
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
