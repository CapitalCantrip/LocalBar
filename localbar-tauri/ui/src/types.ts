export type { ServerType } from './generated/ServerType'
export type { ModelRef } from './generated/ModelRef'
export type { ModelMetadata } from './generated/ModelMetadata'
export type { ParamValue } from './generated/ParamValue'
export type { ParamValues } from './generated/ParamValues'
export type { CanonicalParam } from './generated/CanonicalParam'
export type { ServerInstanceConfig } from './generated/ServerInstanceConfig'
export type { DiscoveryConfig } from './generated/DiscoveryConfig'
export type { DiscoveredModel } from './generated/DiscoveredModel'
export type { ErrorKindDto } from './generated/ErrorKindDto'

import type { InstancePhaseDto as InstancePhase } from './generated/InstancePhaseDto'
export type { InstancePhase }

export function phaseLabel(phase: InstancePhase | undefined): string {
  if (!phase) return 'Unknown'
  switch (phase.type) {
    case 'stopped': return 'Stopped'
    case 'starting': return 'Starting…'
    case 'running': return 'Running'
    case 'stopping': return 'Stopping…'
    case 'switchingModel': return 'Switching model…'
    case 'error': return `Error: ${phase.message}`
  }
}

export function phaseColor(phase: InstancePhase | undefined): string {
  if (!phase) return '#aaa'
  switch (phase.type) {
    case 'running': return '#22c55e'
    case 'starting':
    case 'stopping':
    case 'switchingModel': return '#f59e0b'
    case 'error': return '#ef4444'
    case 'stopped': return '#9ca3af'
  }
}

export function isActive(phase: InstancePhase | undefined): boolean {
  if (!phase) return false
  return phase.type === 'running' || phase.type === 'starting' || phase.type === 'switchingModel'
}
