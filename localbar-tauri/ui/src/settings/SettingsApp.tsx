import { useCallback, useState } from 'react'
import { ipc } from '../ipc'
import { phaseColor } from '../types'
import { useInstances } from '../useInstances'
import { s } from './styles'
import { AddInstanceSheet } from './AddInstanceSheet'
import { DetailPanel } from './DetailPanel'
import { DiscoveryTab } from './DiscoveryTab'

type SettingsTab = 'servers' | 'discovery'

function ignoreModelSelectionFailure() {}

export default function SettingsApp() {
  const [activeTab, setActiveTab] = useState<SettingsTab>('servers')
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const clearSelected = useCallback(() => setSelectedId(null), [])
  const { instances, phases, refresh } = useInstances(clearSelected)
  const [showAddSheet, setShowAddSheet] = useState(false)

  const handleAdd = async (name: string, serverType: string, port: number, execPath: string, selectedModelKey: string | null) => {
    const id = await ipc.addInstance(name, serverType, port, execPath)
    if (selectedModelKey) {
      await ipc.setSelectedModel(id, selectedModelKey).catch(ignoreModelSelectionFailure)
    }
    setShowAddSheet(false)
    await refresh()
    setSelectedId(id)
  }

  const selected = instances.find(i => i.id === selectedId)

  return (
    <div style={{ ...s.root, position: 'relative' }}>
      {showAddSheet && (
        <AddInstanceSheet onAdd={handleAdd} onCancel={() => setShowAddSheet(false)} />
      )}
      <div style={s.toolbar}>
        <button style={s.tabBtn(activeTab === 'servers')} onClick={() => setActiveTab('servers')}>Servers</button>
        <button style={s.tabBtn(activeTab === 'discovery')} onClick={() => setActiveTab('discovery')}>Discovery</button>
      </div>
      {activeTab === 'discovery' ? (
        <DiscoveryTab />
      ) : (
        <div style={s.body}>
          <div style={s.masterPane}>
            <div style={s.masterHeader}>
              <span style={{ fontWeight: 600, fontSize: 12, color: '#444' }}>Instances</span>
              <button style={s.addBtn} onClick={() => setShowAddSheet(true)}>+ Add</button>
            </div>
            <div style={s.masterList}>
              {instances.length === 0 && (
                <p style={s.placeholder}>No servers configured</p>
              )}
              {instances.map(inst => (
                <div
                  key={inst.id}
                  style={s.masterRow(inst.id === selectedId)}
                  onClick={() => setSelectedId(inst.id)}
                >
                  <div style={s.dot(phaseColor(phases[inst.id]))} />
                  <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {inst.name}
                  </span>
                </div>
              ))}
            </div>
          </div>
          <div style={s.detailPane}>
            {selected ? (
              <DetailPanel instance={selected} phase={phases[selected.id]} onRefresh={refresh} />
            ) : (
              <p style={s.placeholder}>Select a server to configure it</p>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
