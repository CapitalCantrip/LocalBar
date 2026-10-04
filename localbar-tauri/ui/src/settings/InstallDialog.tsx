import { useEffect, useState } from 'react'
import { ipc } from '../ipc'
import type { InstallPlanDto } from '../types'
import { s } from './styles'

type InstallState =
  | { step: 'loading' }
  | { step: 'confirm'; plan: InstallPlanDto }
  | { step: 'running'; plan: InstallPlanDto }
  | { step: 'failed'; plan: InstallPlanDto; output: string }
  | { step: 'done'; executable: string }

const codeBlock: React.CSSProperties = {
  fontFamily: 'monospace', fontSize: 11, background: '#f4f4f5', borderRadius: 6,
  padding: 8, margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-all', overflow: 'auto',
}

export function InstallDialog({ instanceId, onClose, onInstalled, onStart }: {
  instanceId: string
  onClose: () => void
  onInstalled: () => void
  onStart: () => void
}) {
  const [state, setState] = useState<InstallState>({ step: 'loading' })

  useEffect(() => {
    let cancelled = false
    ipc.getInstallPlan(instanceId)
      .then(plan => { if (!cancelled) setState({ step: 'confirm', plan }) })
      .catch(e => { if (!cancelled) setState({ step: 'failed', plan: { commands: [], manualUrl: null }, output: String(e) }) })
    return () => { cancelled = true }
  }, [instanceId])

  const runInstall = async (plan: InstallPlanDto) => {
    setState({ step: 'running', plan })
    try {
      const executable = await ipc.installToolForInstance(instanceId)
      setState({ step: 'done', executable })
      onInstalled()
    } catch (e) {
      setState({ step: 'failed', plan, output: String(e) })
    }
  }

  const running = state.step === 'running'

  return (
    <div style={s.sheet}>
      <div style={{ ...s.sheetBox, width: 420 }}>
        <p style={s.sheetTitle}>Install server</p>
        {state.step === 'loading' && <span style={{ fontSize: 12, color: '#666' }}>Checking what is installed…</span>}
        {(state.step === 'confirm' || state.step === 'running' || state.step === 'failed') && state.plan.manualUrl && (
          <>
            <span style={{ fontSize: 12 }}>LocalBar doesn't install this server itself. Install it from the official download page, then press Start again.</span>
            <pre style={codeBlock}>{state.plan.manualUrl}</pre>
          </>
        )}
        {(state.step === 'confirm' || state.step === 'running' || state.step === 'failed') && state.plan.commands.length > 0 && (
          <>
            <span style={{ fontSize: 12 }}>LocalBar will run these commands in your user account (no administrator rights):</span>
            <pre style={codeBlock}>{state.plan.commands.join('\n')}</pre>
          </>
        )}
        {running && <span style={{ fontSize: 12, color: '#666' }}>Installing… this can take a few minutes while packages download.</span>}
        {state.step === 'failed' && (
          <>
            <span style={{ fontSize: 12, color: '#dc2626' }}>The install failed:</span>
            <pre style={{ ...codeBlock, maxHeight: 200, color: '#7f1d1d' }}>{state.output}</pre>
          </>
        )}
        {state.step === 'done' && (
          <span style={{ fontSize: 12 }}>Installed. This instance now runs <code>{state.executable}</code>.</span>
        )}
        <div style={s.sheetBtns}>
          <button style={s.btn()} onClick={onClose} disabled={running}>{state.step === 'done' ? 'Close' : 'Cancel'}</button>
          {(state.step === 'confirm' || state.step === 'failed') && state.plan.manualUrl && (
            <button style={s.primaryBtn} onClick={() => { void ipc.openUrl(state.plan.manualUrl ?? ''); onClose() }}>Open download page</button>
          )}
          {(state.step === 'confirm' || state.step === 'failed' || running) && state.plan.commands.length > 0 && (
            <button style={s.primaryBtn} onClick={() => { if (state.step !== 'running') void runInstall(state.plan) }} disabled={running}>
              {running ? 'Installing…' : state.step === 'failed' ? 'Try again' : 'Install'}
            </button>
          )}
          {state.step === 'done' && (
            <button style={s.primaryBtn} onClick={() => { onStart(); onClose() }}>Start</button>
          )}
        </div>
      </div>
    </div>
  )
}
