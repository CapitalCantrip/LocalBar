import type { CSSProperties } from 'react'
import { ipc, pickPath } from './ipc'
import type { InstancePhase, ServerType } from './types'

export function isExecutableNotFound(phase: InstancePhase | undefined): boolean {
  return phase?.type === 'error' && phase.kind.kind === 'executableNotFound'
}

export function canOfferInstall(serverType: ServerType): boolean {
  return serverType === 'mlx-lm' || serverType === 'ollama'
}

export function MissingExecutableActions({ instanceId, serverType, buttonStyle, onChanged, onInstall }: {
  instanceId: string
  serverType: ServerType
  buttonStyle: CSSProperties
  onChanged: () => void
  onInstall?: () => void
}) {
  const handleChoose = async () => {
    const path = await pickPath({ directory: false })
    if (!path) return
    await ipc.setInstanceExecutablePath(instanceId, path)
    onChanged()
  }

  const handleInstall = () => {
    if (onInstall) onInstall()
    else void ipc.openSettingsForInstance(instanceId, true)
  }

  return (
    <div style={{ display: 'flex', gap: 6 }}>
      <button style={buttonStyle} onClick={handleChoose} title="Pick the server executable for this instance">Choose…</button>
      {canOfferInstall(serverType) && (
        <button style={buttonStyle} onClick={handleInstall} title="Install the server, after showing the commands">Install…</button>
      )}
    </div>
  )
}
