import { openSupportPage } from '../useHostStatus'
import { Button, Note, Panel } from '../room/parts'
import { GLOW, useScreenAt } from '../room/screen'
import type { PageProps } from './page'

/**
 * Support me: WestPlay is free, and its development can be supported on its maker's Kreosus page, which opens
 * in the player's browser. On the room's screen, Home.
 */
export function Support({ cw }: PageProps) {
  const w = cw.support
  useScreenAt('home', GLOW.home, null)

  return (
    <Panel
      place="page"
      title={w.title}
      sub={w.sub}
      action={
        <Button glass onClick={() => void openSupportPage()}>
          {w.button}
        </Button>
      }
    >
      <Note tone="calm">{w.note}</Note>
    </Panel>
  )
}
