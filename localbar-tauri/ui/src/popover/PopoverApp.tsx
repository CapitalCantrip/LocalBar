import { useState } from 'react'
import { ipc } from '../ipc'
import {
  type InstancePhase,
  type ServerInstanceConfig,
  isActive,
  phaseColor,
  phaseLabel,
} from '../types'
import { useInstances } from '../useInstances'
import { startWithWarnings } from '../startWithWarnings'

const s = {
  root: {
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
    fontSize: 13,
    padding: 0,
    background: 'var(--bg, #f5f5f5)',
    color: 'var(--fg, #1a1a1a)',
    minWidth: 280,
    display: 'flex',
    flexDirection: 'column' as const,
  },
  body: { flex: 1, padding: '8px 0' },
  row: {
    display: 'flex',
    alignItems: 'center',
    gap: 8,
    padding: '6px 14px',
    cursor: 'default' as const,
  },
  dot: (color: string): React.CSSProperties => ({
    width: 8, height: 8, borderRadius: '50%', background: color, flexShrink: 0,
  }),
  name: { flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' as const },
  label: { fontSize: 11, color: '#666', flexShrink: 0, maxWidth: 90, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' as const },
  btn: (primary?: boolean): React.CSSProperties => ({
    padding: '3px 8px', border: '1px solid #ccc', borderRadius: 4,
    background: primary ? '#1d4ed8' : 'white',
    color: primary ? 'white' : '#1a1a1a',
    cursor: 'pointer', fontSize: 11, fontFamily: 'inherit', flexShrink: 0,
  }),
  empty: {
    display: 'flex', flexDirection: 'column' as const,
    alignItems: 'center', justifyContent: 'center',
    padding: '24px 16px', gap: 4,
  },
  divider: { height: 1, background: '#ddd', margin: 0 },
  footer: { display: 'flex', gap: 8, padding: '10px 12px' },
  footerBtn: {
    flex: 1, padding: '5px 10px', border: '1px solid #ccc', borderRadius: 6,
    background: 'white', cursor: 'pointer', fontSize: 12, fontFamily: 'inherit',
  },
  dialog: {
    position: 'absolute' as const, inset: 0,
    background: 'rgba(0,0,0,0.4)', display: 'flex',
    alignItems: 'center', justifyContent: 'center', zIndex: 10,
  },
  dialogBox: {
    background: 'white', borderRadius: 8, padding: 16,
    maxWidth: 260, boxShadow: '0 4px 20px rgba(0,0,0,0.25)',
  },
  dialogMsg: { fontSize: 12, lineHeight: 1.5, marginBottom: 12 },
  dialogRow: { display: 'flex', gap: 8, justifyContent: 'flex-end' },
}

function ConfirmDialog({ message, onConfirm, onCancel }: {
  message: string
  onConfirm: () => void
  onCancel: () => void
}) {
  return (
    <div style={s.dialog}>
      <div style={s.dialogBox}>
        <p style={s.dialogMsg}>{message}</p>
        <div style={s.dialogRow}>
          <button style={s.btn()} onClick={onCancel}>Cancel</button>
          <button style={s.btn(true)} onClick={onConfirm}>Start Anyway</button>
        </div>
      </div>
    </div>
  )
}

function InstanceRow({ instance, phase, onStart, onStop }: {
  instance: ServerInstanceConfig
  phase: InstancePhase | undefined
  onStart: () => void
  onStop: () => void
}) {
  const active = isActive(phase)
  const transitioning = phase?.type === 'starting' || phase?.type === 'stopping'
  return (
    <div style={s.row}>
      <div style={s.dot(phaseColor(phase))} title={phaseLabel(phase)} />
      <span style={s.name} title={instance.name}>{instance.name}</span>
      <span style={s.label}>{phaseLabel(phase)}</span>
      {active && !transitioning && (
        <button style={s.btn()} onClick={onStop}>Stop</button>
      )}
      {!active && !transitioning && (
        <button style={s.btn(true)} onClick={onStart}>Start</button>
      )}
    </div>
  )
}

export default function PopoverApp() {
  const { instances, phases, refresh } = useInstances()
  const [confirm, setConfirm] = useState<{ id: string; msg: string; resolve: (v: boolean) => void } | null>(null)

  const promptWarning = (id: string, msg: string): Promise<boolean> =>
    new Promise(resolve => setConfirm({ id, msg, resolve }))

  const handleStart = async (id: string) => {
    const started = await startWithWarnings(id, msg => promptWarning(id, msg))
    if (started) refresh()
  }

  const handleStop = async (id: string) => {
    await ipc.stopInstance(id)
    refresh()
  }

  const anyRunning = instances.some(i => isActive(phases[i.id]))

  return (
    <div style={{ ...s.root, position: 'relative' }}>
      {confirm && (
        <ConfirmDialog
          message={confirm.msg}
          onConfirm={() => { confirm.resolve(true); setConfirm(null) }}
          onCancel={() => { confirm.resolve(false); setConfirm(null) }}
        />
      )}
      <div style={s.body}>
        {instances.length === 0 ? (
          <div style={s.empty}>
            <p style={{ fontWeight: 600, margin: 0 }}>No instances configured</p>
            <p style={{ fontSize: 11, color: '#888', margin: 0 }}>Open Settings to add one.</p>
          </div>
        ) : (
          instances.map(inst => (
            <InstanceRow
              key={inst.id}
              instance={inst}
              phase={phases[inst.id]}
              onStart={() => handleStart(inst.id)}
              onStop={() => handleStop(inst.id)}
            />
          ))
        )}
      </div>
      <div style={s.divider} />
      <div style={s.footer}>
        <button style={s.footerBtn} onClick={() => ipc.openSettings()}>Settings…</button>
        {anyRunning && (
          <button
            style={s.footerBtn}
            onClick={async () => {
              await Promise.all(
                instances.filter(i => isActive(phases[i.id])).map(i => ipc.stopInstance(i.id))
              )
              refresh()
            }}
          >
            Stop All
          </button>
        )}
        <button style={s.footerBtn} onClick={() => ipc.quitApp()}>Quit</button>
      </div>
    </div>
  )
}
