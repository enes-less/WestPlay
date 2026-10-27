import { useEffect, useState } from 'react'
import { Button, Checking, Group, Note, Panel, Row, Status, Tick } from '../room/parts'
import { banner, explain, gameSummary, useGameRunning, useWords } from '../room/room'
import { GLOW, useScreen, useScreenAt } from '../room/screen'
import { checkGame, type InstallReport, installIntoGame } from '../setup'
import { type PageProps, useGameCheck } from './page'

/**
 * Where WestPlay is installed and how to put it right: the game it's in, the plugin as it is
 * there, and a reinstall (the onboarding's install, again, while the game is closed). On the room's
 * screen, the check's scan while the game is looked at, and WestPlay booting again after a reinstall.
 */
export function Setup(props: PageProps) {
  const { setup, cw, check, checked, uninstall, tutorial } = props
  const screen = useScreen()
  const words = useWords()
  const w = cw.setup
  const error = useGameCheck(props)
  const running = useGameRunning()
  const [installing, setInstalling] = useState(false)
  const [report, setReport] = useState<InstallReport | null>(null)
  const [installError, setInstallError] = useState<string | null>(null)
  const game = setup.game

  useEffect(() => {
    screen.scan(!check && !error)
    return () => screen.scan(false)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [check, error])
  useScreenAt(
    'home',
    check ? GLOW.home : GLOW.scan,
    installing
      ? banner({ ...words.install.banners.installing, text: words.install.plugin }, true)
      : report?.done
        ? banner({ title: w.reinstalled, text: words.install.pluginDetail })
        : check
          ? banner(w.checked)
          : error
            ? null
            : banner(w.checking, true),
  )

  const reinstall = async (administrator: boolean) => {
    if (!game) return
    setInstalling(true)
    setInstallError(null)
    try {
      const result = await installIntoGame(game.dir, administrator)
      setReport(result)
      if (result.done) {
        screen.reboot()
        // Checked again: the plugin and the mod as they are now.
        checkGame(game.dir).then(checked, () => {})
      }
    } catch (e) {
      setInstallError(explain(words, e))
    } finally {
      setInstalling(false)
    }
  }

  const plugin = report?.plugin
  const asAdministrator = plugin?.state === 'needsAdministrator' || plugin?.state === 'declined'
  const pluginState = check?.plugin
  return (
    <Panel
      place="page"
      title={w.title}
      sub={w.sub}
      action={
        game && (
          <Button glass onClick={() => reinstall(asAdministrator)} busy={installing} disabled={running !== false}>
            {asAdministrator ? words.install.asAdministrator : w.reinstall}
          </Button>
        )
      }
    >
      {game && (
        <Group title={w.game} place={4}>
          <Row title={w.folder} detail={game.dir} />
          <Row
            title={words.check.version}
            detail={gameSummary(words, game)}
            accessory={game.tested ? <Tick done /> : undefined}
          />
        </Group>
      )}
      <Group title={w.installed} place={5}>
        <Row
          title={w.plugin}
          detail={words.install.pluginDetail}
          value={pluginState ? w.pluginStates[pluginState] : undefined}
          accessory={
            !check && !error ? (
              <Checking />
            ) : pluginState === 'current' ? (
              <Tick done />
            ) : (
              <Status tone={pluginState === 'missing' ? 'bad' : 'idle'} />
            )
          }
        />
      </Group>
      {check?.modProblem && (
        <Note tone="bad">
          <b>{words.check.cantInstall}</b> {check.modProblem}
        </Note>
      )}
      {check?.otherApps.includes('ymnOne') && (
        <Note tone="bad">
          <b>{words.check.otherCarPlay.title}</b> {words.check.otherCarPlay.text}
        </Note>
      )}
      {running && (
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
          <b>{words.install.inUse.title}</b> {words.install.inUse.text}
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
      {(installError ?? error) && <Note tone="bad">{installError ?? error}</Note>}
      <Group place={6}>
        <Row title={w.tutorial} link onClick={tutorial} />
        <Row title={cw.uninstall.row} danger onClick={uninstall} />
      </Group>
    </Panel>
  )
}
