import type { CSSProperties } from 'react'
import { ipc, pickPath } from './ipc'
import type { InstancePhase } from './types'

export function isExecutableNotFound(phase: InstancePhase | undefined): boolean {
  return phase?.type === 'error' && phase.kind.kind === 'executableNotFound'
}

export function MissingExecutableActions({ instanceId, buttonStyle, onChanged }: {
  instanceId: string
  buttonStyle: CSSProperties
  onChanged: () => void
}) {
  const handleChoose = async () => {
    const path = await pickPath({ directory: false })
    if (!path) return
    await ipc.setInstanceExecutablePath(instanceId, path)
    onChanged()
  }

  return (
    <div style={{ display: 'flex', gap: 6 }}>
      <button style={buttonStyle} onClick={handleChoose} title="Pick the server executable for this instance">Choose…</button>
    </div>
  )
}
