import { ipc } from '../ipc'

const styles = {
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
  body: {
    flex: 1,
    padding: '16px 16px 8px',
    display: 'flex',
    flexDirection: 'column' as const,
    alignItems: 'center',
    justifyContent: 'center',
    minHeight: 100,
  },
  emptyHeading: {
    fontWeight: 600,
    marginBottom: 4,
  },
  emptyCaption: {
    fontSize: 11,
    color: '#888',
  },
  divider: {
    height: 1,
    background: '#ddd',
    margin: 0,
  },
  footer: {
    display: 'flex',
    gap: 8,
    padding: '10px 12px',
  },
  btn: {
    flex: 1,
    padding: '5px 10px',
    border: '1px solid #ccc',
    borderRadius: 6,
    background: 'white',
    cursor: 'pointer',
    fontSize: 12,
    fontFamily: 'inherit',
  },
}

export default function PopoverApp() {
  return (
    <div style={styles.root}>
      <div style={styles.body}>
        <p style={styles.emptyHeading}>No instances configured</p>
        <p style={styles.emptyCaption}>Open Settings to add one.</p>
      </div>
      <div style={styles.divider} />
      <div style={styles.footer}>
        <button style={styles.btn} onClick={() => ipc.openSettings()}>
          Settings…
        </button>
        <button style={styles.btn} onClick={() => ipc.quitApp()}>
          Quit
        </button>
      </div>
    </div>
  )
}
