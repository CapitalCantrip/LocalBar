import { invoke } from '@tauri-apps/api/core'
import type { DiscoveredModel, DiscoveryConfig, InstancePhase, ModelMetadata, ModelRef, ParamValues, ServerInstanceConfig } from './types'

export interface ParamSchemaEntry {
  key: string
  label: string
  kind: 'double' | 'int'
  server_flag: string
  default_value: { type: 'double' | 'int'; value: number } | null
}

export const ipc = {
  listInstances: (): Promise<ServerInstanceConfig[]> =>
    invoke('list_instances'),

  listInstancePhases: (): Promise<Record<string, InstancePhase>> =>
    invoke('list_instance_phases'),

  addInstance: (
    name: string,
    serverType: string,
    port: number,
    executablePath: string,
  ): Promise<string> =>
    invoke('add_instance', { name, serverType, port, executablePath }),

  removeInstance: (id: string): Promise<void> =>
    invoke('remove_instance', { id }),

  getStartWarning: (id: string): Promise<string | null> =>
    invoke('get_start_warning', { id }),

  checkMemoryWarning: (id: string): Promise<string | null> =>
    invoke('check_memory_warning', { id }),

  startInstance: (id: string): Promise<void> =>
    invoke('start_instance', { id }),

  stopInstance: (id: string): Promise<void> =>
    invoke('stop_instance', { id }),

  setStartOnLaunch: (id: string, value: boolean): Promise<void> =>
    invoke('set_start_on_launch', { id, value }),

  setSelectedModel: (id: string, modelKey: string | null): Promise<void> =>
    invoke('set_selected_model', { id, modelKey }),

  listModels: (id: string): Promise<ModelRef[]> =>
    invoke('list_models_cmd', { id }),

  listModelsForType: (serverType: string): Promise<ModelRef[]> =>
    invoke('list_models_for_type', { serverType }),

  switchModel: (id: string, modelKey: string): Promise<void> =>
    invoke('switch_model_cmd', { id, modelKey }),

  fetchModelMetadata: (id: string, modelKey: string): Promise<ModelMetadata | null> =>
    invoke('fetch_model_metadata_cmd', { id, modelKey }),

  updateInstanceParams: (id: string, params: ParamValues): Promise<void> =>
    invoke('update_instance_params', { id, params }),

  getResolvedParams: (id: string): Promise<ParamValues> =>
    invoke('get_resolved_params', { id }),

  getParamSchema: (serverType: string): Promise<ParamSchemaEntry[]> =>
    invoke('get_param_schema', { serverType }),

  listAllDiscoveredModels: (): Promise<DiscoveredModel[]> =>
    invoke('list_all_discovered_models'),

  getDiscoveryConfig: (): Promise<DiscoveryConfig> =>
    invoke('get_discovery_config'),

  setDiscoveryConfig: (config: DiscoveryConfig): Promise<void> =>
    invoke('set_discovery_config', { config }),

  setModelSearchPathOverride: (id: string, path: string | null): Promise<void> =>
    invoke('set_model_search_path_override', { id, path }),

  openSettings: (): Promise<void> =>
    invoke('open_settings'),

  quitApp: (): Promise<void> =>
    invoke('quit_app'),
}
