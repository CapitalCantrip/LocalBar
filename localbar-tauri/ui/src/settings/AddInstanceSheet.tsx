import { useCallback, useEffect, useRef, useState } from 'react'
import { ipc, pickPath } from '../ipc'
import type { MlxDetectionDto } from '../types'
import { useDetectionProgress } from './useDetectionProgress'
import { s } from './styles'
import { AddModelPicker, PICKER_TYPES } from './AddModelPicker'

export type ServerTypeOption = 'ollama' | 'mlx-lm' | 'external'

export const DEFAULTS: Record<ServerTypeOption, { port: string; execPath: string; namePlaceholder: string }> = {
  ollama: { port: '11434', execPath: '/usr/local/bin/ollama', namePlaceholder: 'My Ollama' },
  'mlx-lm': { port: '8080', execPath: 'uvx', namePlaceholder: 'My mlx-lm' },
  external: { port: '11434', execPath: '', namePlaceholder: 'My External Server' },
}

const linkBtn: React.CSSProperties = {
  background: 'none', border: 'none', padding: 0, color: '#2563eb', cursor: 'pointer', fontSize: 11,
}

export function AddInstanceSheet({ onAdd, onCancel }: {
  onAdd: (name: string, serverType: string, port: number, execPath: string, selectedModelKey: string | null, modelFolder: string | null) => Promise<void>
  onCancel: () => void
}) {
  const [name, setName] = useState('')
  const [serverType, setServerType] = useState<ServerTypeOption>('ollama')
  const [port, setPort] = useState(DEFAULTS.ollama.port)
  const [execPath, setExecPath] = useState(DEFAULTS.ollama.execPath)
  const [selectedModelKey, setSelectedModelKey] = useState<string | null>(null)
  const [modelFolder, setModelFolder] = useState('')
  const [pickerFolder, setPickerFolder] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [detecting, setDetecting] = useState(false)
  const [detection, setDetection] = useState<MlxDetectionDto | null>(null)
  const detectionRequest = useRef<string | null>(null)
  const execPathEdited = useRef(false)
  const progress = useDetectionProgress(detecting)

  const stopDetection = useCallback(() => {
    const requestId = detectionRequest.current
    detectionRequest.current = null
    setDetecting(false)
    if (requestId !== null) void ipc.cancelMlxDetection(requestId)
  }, [])

  const startDetection = useCallback((force: boolean) => {
    stopDetection()
    const requestId = crypto.randomUUID()
    detectionRequest.current = requestId
    setDetecting(true)
    setDetection(null)
    ipc.detectMlxLauncher(requestId, force)
      .then(result => {
        if (requestId !== detectionRequest.current) return
        setDetection(result)
        const found = result.executable
        if (found !== null && !execPathEdited.current) setExecPath(found)
      })
      .catch(() => {})
      .finally(() => {
        if (requestId !== detectionRequest.current) return
        detectionRequest.current = null
        setDetecting(false)
      })
  }, [stopDetection])

  useEffect(() => {
    if (serverType !== 'mlx-lm') return
    execPathEdited.current = false
    startDetection(false)
    return stopDetection
  }, [serverType, startDetection, stopDetection])

  const editExecPath = (value: string) => {
    execPathEdited.current = true
    if (detecting) stopDetection()
    setExecPath(value)
  }

  const selectType = (t: ServerTypeOption) => {
    setServerType(t)
    setPort(DEFAULTS[t].port)
    setExecPath(DEFAULTS[t].execPath)
    setSelectedModelKey(null)
    setModelFolder('')
    setPickerFolder(null)
  }

  const applyFolder = (folder: string) => {
    setModelFolder(folder)
    setPickerFolder(folder.trim() || null)
    setSelectedModelKey(null)
  }

  const chooseFolder = async () => {
    const picked = await pickPath({ directory: true })
    if (picked) applyFolder(picked)
  }

  const folderForType = PICKER_TYPES.includes(serverType) ? (modelFolder.trim() || null) : null

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim() || (serverType !== 'external' && !execPath.trim())) return
    setBusy(true)
    try {
      await onAdd(name.trim(), serverType, parseInt(port, 10) || parseInt(DEFAULTS[serverType].port, 10), execPath.trim(), selectedModelKey, folderForType)
    } finally { setBusy(false) }
  }

  return (
    <div style={s.sheet}>
      <form style={s.sheetBox} onSubmit={submit}>
        <p style={s.sheetTitle}>Add Instance</p>
        <div style={s.field}>
          <span style={s.fieldLabel}>Server type</span>
          <div style={{ display: 'flex', gap: 16, marginTop: 2 }}>
            {(['ollama', 'mlx-lm', 'external'] as const).map(t => (
              <label key={t} style={{ display: 'flex', alignItems: 'center', gap: 5, cursor: 'pointer', fontSize: 13 }}>
                <input type="radio" name="serverType" value={t} checked={serverType === t} onChange={() => selectType(t)} />
                {t}
              </label>
            ))}
          </div>
        </div>
        <div style={s.field}>
          <label style={s.fieldLabel}>Name</label>
          <input style={s.input} value={name} onChange={e => setName(e.target.value)} placeholder={DEFAULTS[serverType].namePlaceholder} autoFocus />
        </div>
        <div style={s.field}>
          <label style={s.fieldLabel}>Port</label>
          <input style={s.input} value={port} onChange={e => setPort(e.target.value)} />
        </div>
        {serverType !== 'external' && (
          <div style={s.field}>
            <label style={s.fieldLabel}>Executable path</label>
            <input style={s.input} value={execPath} onChange={e => editExecPath(e.target.value)} />
            {serverType === 'mlx-lm' && detecting && (
              <span style={{ fontSize: 11, color: '#666', display: 'flex', gap: 8, alignItems: 'baseline' }}>
                <span style={{ flex: 1, wordBreak: 'break-all' }}>{progress ?? 'Looking for mlx-lm…'}</span>
                <button type="button" style={linkBtn} onClick={stopDetection}>Skip</button>
              </span>
            )}
            {serverType === 'mlx-lm' && !detecting && (
              <span style={{ fontSize: 11, color: '#666' }}>
                {detection && detection.kind !== 'cancelled' && <>{detection.message} </>}
                <button type="button" style={linkBtn} onClick={() => startDetection(true)}>Detect again</button>
              </span>
            )}
          </div>
        )}
        {PICKER_TYPES.includes(serverType) && (
          <div style={s.field}>
            <label style={s.fieldLabel}>Model folder</label>
            <div style={{ display: 'flex', gap: 6 }}>
              <input
                style={{ ...s.input, flex: 1 }}
                value={modelFolder}
                onChange={e => setModelFolder(e.target.value)}
                onBlur={e => applyFolder(e.target.value)}
                placeholder="Optional — defaults to Settings → Discovery"
              />
              <button type="button" style={s.btn()} onClick={() => void chooseFolder()}>Choose…</button>
            </div>
          </div>
        )}
        <AddModelPicker serverType={serverType} modelFolder={pickerFolder} selectedModelKey={selectedModelKey} onSelect={setSelectedModelKey} />
        <div style={s.sheetBtns}>
          <button type="button" style={s.btn()} onClick={onCancel}>Cancel</button>
          <button type="submit" style={s.primaryBtn} disabled={busy || !name.trim()}>
            {busy ? 'Adding…' : 'Add'}
          </button>
        </div>
      </form>
    </div>
  )
}
