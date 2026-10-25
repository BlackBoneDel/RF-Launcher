import { invoke } from '@tauri-apps/api/tauri';
import type { ConfigKey, ConfigValue, Instance, KillResult, LaunchResult, RamCheck } from '../types';

export async function getTheme(): Promise<string> {
  return invoke('get_theme');
}

export async function setTheme(theme: string): Promise<void> {
  return invoke('set_theme', { theme });
}

export async function listInstances(): Promise<Instance[]> {
  return invoke('list_instances');
}

export async function createInstance(values: {
  name: string;
  minecraft_version: string;
  fabric_loader: string | null;
}): Promise<Instance> {
  return invoke('create_instance', values);
}

export async function getInstance(id: string): Promise<Instance> {
  return invoke('get_instance', { id });
}

export async function resolveConfig(instanceId: string, key: ConfigKey): Promise<ConfigValue> {
  return invoke('resolve_config', { instance_id: instanceId, key });
}

export async function setGlobalConfig(key: ConfigKey, value: ConfigValue): Promise<void> {
  return invoke('set_global_config', { key, value });
}

export async function setInstanceConfig(
  instanceId: string,
  key: ConfigKey,
  value: ConfigValue,
  force = false,
): Promise<RamCheck> {
  return invoke('set_instance_config', { instance_id: instanceId, key, value, force });
}

export async function getSystemRam(): Promise<RamCheck> {
  return invoke('get_system_ram');
}

export async function launchInstance(id: string): Promise<LaunchResult> {
  return invoke('launch_instance', { id });
}

export async function killInstance(id: string): Promise<KillResult> {
  return invoke('kill_instance', { id });
}
