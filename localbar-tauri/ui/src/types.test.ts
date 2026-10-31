import { describe, expect, it } from 'vitest'
import { isActive, phaseColor, phaseLabel, pidLabel } from './types'
import type { InstancePhase } from './types'

describe('phaseLabel', () => {
  it('says Unknown when there is no phase', () => {
    expect(phaseLabel(undefined)).toBe('Unknown')
  })

  it.each([
    [{ type: 'stopped' }, 'Stopped'],
    [{ type: 'starting' }, 'Starting…'],
    [{ type: 'running' }, 'Running'],
    [{ type: 'stopping' }, 'Stopping…'],
    [{ type: 'switchingModel' }, 'Switching model…'],
  ] as [InstancePhase, string][])('labels %j as %s', (phase, label) => {
    expect(phaseLabel(phase)).toBe(label)
  })

  it('includes the message when the instance is in error', () => {
    expect(phaseLabel({ type: 'error', message: 'port busy' } as InstancePhase)).toBe('Error: port busy')
  })
})

describe('phaseColor', () => {
  it('is grey-ish when there is no phase', () => {
    expect(phaseColor(undefined)).toBe('#aaa')
  })

  it('is green only for running', () => {
    expect(phaseColor({ type: 'running' } as InstancePhase)).toBe('#22c55e')
  })

  it.each(['starting', 'stopping', 'switchingModel'])('is amber while %s', type => {
    expect(phaseColor({ type } as InstancePhase)).toBe('#f59e0b')
  })

  it('is red on error', () => {
    expect(phaseColor({ type: 'error', message: 'x' } as InstancePhase)).toBe('#ef4444')
  })

  it('is grey when stopped', () => {
    expect(phaseColor({ type: 'stopped' } as InstancePhase)).toBe('#9ca3af')
  })
})

describe('isActive', () => {
  it('is false when there is no phase', () => {
    expect(isActive(undefined)).toBe(false)
  })

  it.each(['running', 'starting', 'switchingModel'])('is true while %s', type => {
    expect(isActive({ type } as InstancePhase)).toBe(true)
  })

  it.each(['stopped', 'stopping'])('is false while %s', type => {
    expect(isActive({ type } as InstancePhase)).toBe(false)
  })

  it('is false on error', () => {
    expect(isActive({ type: 'error', message: 'x' } as InstancePhase)).toBe(false)
  })
})

describe('pidLabel', () => {
  it('shows a dash for a missing pid', () => {
    expect(pidLabel(null)).toBe('—')
    expect(pidLabel(undefined)).toBe('—')
  })

  it('shows the bare pid for a process we started', () => {
    expect(pidLabel({ pid: 42, adopted: false })).toBe('42')
  })

  it('marks an adopted pid', () => {
    expect(pidLabel({ pid: 42, adopted: true })).toBe('42 (adopted)')
  })
})
