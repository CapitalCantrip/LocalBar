// TypeScript mirrors of Rust types from localbar-core.
// These must stay in sync with the Rust definitions in localbar-core/src/types.rs.

export type ServerType = 'mlx-lm' | 'ollama' | 'external'

export interface ModelRef {
  key: string
  display_name: string
  publisher: string | null
  architecture: string | null
  size_bytes: number | null
  modified_secs: number | null
}

export interface ModelMetadata {
  parameter_count: string | null
  quantization: string | null
}

export interface ParamValue {
  type: 'double' | 'int' | 'string' | 'bool'
  value: number | string | boolean
}

export interface ParamValues {
  values: Record<string, ParamValue>
  system_prompt: string | null
}

export interface ServerInstanceConfig {
  id: string
  name: string
  server_type: ServerType
  host: string
  port: number
  executable_path: string
  selected_model_key: string | null
  instance_params: ParamValues
  active_profile_id: string | null
  managed_model_tag: string | null
  start_on_launch: boolean
  was_running_when_quit: boolean
  model_search_path_override: string | null
}

export interface DiscoveryConfig {
  mlx_lm_search_paths: string[]
  ollama_executable_path: string | null
}

export interface DiscoveredModel {
  server_type: string
  key: string
  display_name: string
  publisher: string | null
  architecture: string | null
  parameter_count: string | null
  quantization: string | null
  size_bytes: number | null
  modified_secs: number | null
}

export type InstancePhase =
  | { type: 'stopped' }
  | { type: 'starting' }
  | { type: 'running' }
  | { type: 'stopping' }
  | { type: 'switchingModel' }
  | { type: 'error'; message: string }

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
