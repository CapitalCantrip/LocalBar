import { describe, expect, it } from 'vitest'
import { checkResultLabel } from './GeneralTab'
import { fmtBytes } from './DiscoveredModelsSection'
import type { UpdateCheckOutcome } from '../types'

describe('checkResultLabel', () => {
  it('reassures when up to date', () => {
    expect(checkResultLabel({ type: 'upToDate' } as UpdateCheckOutcome)).toBe("You're up to date")
  })

  it('reports a failed check', () => {
    expect(checkResultLabel({ type: 'failed' } as UpdateCheckOutcome)).toBe("Couldn't check")
  })

  it('has no label when an update is available', () => {
    expect(checkResultLabel({ type: 'available' } as UpdateCheckOutcome)).toBeNull()
  })
})

describe('fmtBytes', () => {
  it('shows a dash for unknown size', () => {
    expect(fmtBytes(null)).toBe('—')
  })

  it.each([
    [512, '512 B'],
    [2_000, '2 KB'],
    [5_000_000, '5 MB'],
    [1_500_000_000, '1.5 GB'],
  ])('formats %d as %s', (bytes, text) => {
    expect(fmtBytes(bytes)).toBe(text)
  })
})
