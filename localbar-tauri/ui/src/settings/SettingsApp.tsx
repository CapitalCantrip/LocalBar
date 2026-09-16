import { useState } from 'react'

type Tab = 'servers'

const styles = {
  root: {
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
    fontSize: 13,
    height: '100vh',
    display: 'flex',
    flexDirection: 'column' as const,
    background: '#f0f0f0',
    color: '#1a1a1a',
  },
  toolbar: {
    display: 'flex',
    alignItems: 'center',
    gap: 4,
    padding: '8px 12px',
    borderBottom: '1px solid #ddd',
    background: '#e8e8e8',
  },
  tab: (active: boolean): React.CSSProperties => ({
    padding: '4px 14px',
    borderRadius: 6,
    border: '1px solid transparent',
    background: active ? 'white' : 'transparent',
    boxShadow: active ? '0 1px 3px rgba(0,0,0,0.15)' : 'none',
    cursor: 'pointer',
    fontSize: 13,
    fontFamily: 'inherit',
  }),
  body: {
    flex: 1,
    display: 'flex',
    overflow: 'hidden',
  },
  masterPane: {
    width: 220,
    borderRight: '1px solid #ddd',
    background: 'white',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
  },
  detailPane: {
    flex: 1,
    background: 'white',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
  },
  placeholder: {
    color: '#aaa',
    fontSize: 12,
    textAlign: 'center' as const,
  },
}

export default function SettingsApp() {
  const [activeTab] = useState<Tab>('servers')

  return (
    <div style={styles.root}>
      <div style={styles.toolbar}>
        <button style={styles.tab(activeTab === 'servers')}>Servers</button>
      </div>
      <div style={styles.body}>
        <div style={styles.masterPane}>
          <p style={styles.placeholder}>No servers configured</p>
        </div>
        <div style={styles.detailPane}>
          <p style={styles.placeholder}>Select a server to configure it</p>
        </div>
      </div>
    </div>
  )
}
