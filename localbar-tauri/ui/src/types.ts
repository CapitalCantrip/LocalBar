// TypeScript mirrors of Rust types from localbar-core.
// These must stay in sync with the Rust definitions in localbar-core/src/types.rs.

export type ServerType = 'mlx-lm' | 'ollama'

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
}

export type InstancePhase =
  | { type: 'stopped' }
  | { type: 'starting' }
  | { type: 'running' }
  | { type: 'stopping' }
  | { type: 'switchingModel' }
  | { type: 'error'; message: string }
