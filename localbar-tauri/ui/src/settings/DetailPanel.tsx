import { useState } from 'react'
import { ipc } from '../ipc'
import {
  type InstancePhase,
  type InstancePidDto,
  type ServerInstanceConfig,
  isActive,
  phaseColor,
  phaseLabel,
  pidLabel,
} from '../types'
import { startWithWarnings } from '../startWithWarnings'
import { modelLabelForKey } from '../modelLabel'
import { s } from './styles'
import { InlineEdit } from './InlineEdit'
import { ModelPathOverrideField } from './ModelPathOverrideField'
import { ModelList } from './ModelList'
import { ParamEditor } from './ParamEditor'
import { useModelList } from './useModelList'

export function DetailPanel({ instance, phase, pid, onRefresh }: {
  instance: ServerInstanceConfig
  phase: InstancePhase | undefined
  pid: InstancePidDto | null | undefined
  onRefresh: () => void
}) {
  const active = isActive(phase)
  const transitioning = phase?.type === 'starting' || phase?.type === 'stopping'
  const showPid = phase?.type === 'running' || phase?.type === 'switchingModel'
  const isPortConflict = phase?.type === 'error' && phase.kind.kind === 'portConflict'
  const [confirmingRemove, setConfirmingRemove] = useState(false)
  const [modelsOpen, setModelsOpen] = useState(false)
  const { models, loading: modelsLoading, error: modelsError, refresh: refreshModels } = useModelList(instance.id)

  const handleStart = async () => {
    await startWithWarnings(instance.id, async msg => {
      setConfirmingRemove(false)
      return new Promise(resolve => {
        const ok = window.confirm(msg + '\n\nStart anyway?')
        resolve(ok)
      })
    })
    onRefresh()
  }

  const handleStop = async () => {
    await ipc.stopInstance(instance.id)
    onRefresh()
  }

  const handleAdopt = async () => {
    const { warning } = await ipc.adoptAsExternalInstance(instance.id)
    onRefresh()
    if (warning) window.alert(warning)
  }

  const handleRemove = async () => {
    setConfirmingRemove(false)
    await ipc.removeInstance(instance.id)
  }

  const endpoint = `http://localhost:${instance.port}`

  return (
    <div style={s.detailPane}>
      <div style={s.detailHeader}>
        <InlineEdit
          value={instance.name}
          onSave={async v => { await ipc.renameInstance(instance.id, v); onRefresh() }}
          style={{ fontWeight: 600, fontSize: 15, border: 'none', padding: '2px 4px', borderRadius: 4, background: 'transparent', width: 'auto', flex: 1 }}
        />
        {confirmingRemove ? (
          <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
            <span style={{ fontSize: 12, color: '#c00' }}>Remove?</span>
            <button style={s.btn(true)} onClick={handleRemove}>Yes</button>
            <button style={s.btn()} onClick={() => setConfirmingRemove(false)}>No</button>
          </div>
        ) : (
          <button style={s.btn(true)} onClick={() => setConfirmingRemove(true)}>Remove</button>
        )}
      </div>
      <div style={s.detailBody}>
        <div style={s.field}>
          <span style={s.fieldLabel}>Phase</span>
          <div style={s.phaseRow}>
            <div style={s.dot(phaseColor(phase))} />
            <span style={s.fieldValue}>{phaseLabel(phase)}</span>
          </div>
        </div>
        <div style={s.btnRow}>
          {active && !transitioning && (
            <button style={s.btn()} onClick={handleStop}>Stop</button>
          )}
          {!active && !transitioning && (
            <button
              style={s.primaryBtn}
              onClick={handleStart}
              disabled={(instance.server_type === 'mlx-lm' || instance.server_type === 'ollama') && instance.selected_model_key === null}
            >Start</button>
          )}
          {isPortConflict && (
            <button
              style={s.btn()}
              onClick={handleAdopt}
              title="Track the server already running on this port as a new external instance"
            >Adopt</button>
          )}
        </div>
        <div style={s.field}>
          <span style={s.fieldLabel}>Port</span>
          <InlineEdit
            value={String(instance.port)}
            onSave={async v => {
              const n = parseInt(v, 10)
              if (!isNaN(n) && n > 0 && n < 65536) { await ipc.setInstancePort(instance.id, n); onRefresh() }
            }}
            inputStyle={{ width: 90 }}
          />
        </div>
        <div style={s.field}>
          <span style={s.fieldLabel}>Endpoint</span>
          <span style={{ ...s.fieldValue, fontSize: 11, color: '#555', fontFamily: 'monospace' }}>{endpoint}</span>
        </div>
        {showPid && (
          <div style={s.field}>
            <span style={s.fieldLabel}>PID</span>
            <span style={{ ...s.fieldValue, fontSize: 11, color: '#555', fontFamily: 'monospace' }}>{pidLabel(pid)}</span>
          </div>
        )}
        <div style={s.field}>
          <span style={s.fieldLabel}>Model</span>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <span
              style={{ ...s.fieldValue, fontSize: 11, flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
              title={instance.selected_model_key ?? undefined}
            >
              {instance.selected_model_key === null
                ? <span style={{ color: '#aaa' }}>None selected</span>
                : modelLabelForKey(instance.selected_model_key, models)}
            </span>
            <button
              style={{ ...s.btn(), padding: '2px 8px', fontSize: 11 }}
              onClick={() => setModelsOpen(o => !o)}
            >{modelsOpen ? '▲ Models' : '▼ Models'}</button>
          </div>
          {modelsOpen && (
            <div style={{ marginTop: 8, display: 'flex', flexDirection: 'column', gap: 10 }}>
              {instance.server_type === 'mlx-lm' && (
                <ModelPathOverrideField instance={instance} onRefresh={onRefresh} />
              )}
              <ModelList
                instance={instance}
                phase={phase}
                models={models}
                loading={modelsLoading}
                error={modelsError}
                onRefetch={refreshModels}
                onRefresh={onRefresh}
              />
            </div>
          )}
        </div>
        <div style={s.field}>
          <span style={s.fieldLabel}>Server type</span>
          <span style={s.fieldValue}>{instance.server_type}</span>
        </div>
        <div style={s.field}>
          <span style={s.fieldLabel}>Executable</span>
          <span style={{ ...s.fieldValue, wordBreak: 'break-all', fontSize: 11 }}>{instance.executable_path}</span>
        </div>
        <ParamEditor instance={instance} onRefresh={onRefresh} />
        <label style={s.checkbox}>
          <input
            type="checkbox"
            checked={instance.start_on_launch}
            onChange={async e => {
              await ipc.setStartOnLaunch(instance.id, e.target.checked)
              onRefresh()
            }}
          />
          Start on app launch
        </label>
      </div>
    </div>
  )
}
