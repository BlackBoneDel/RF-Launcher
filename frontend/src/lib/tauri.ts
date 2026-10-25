import { invoke } from '@tauri-apps/api/tauri';
import type { ConfigKey, ConfigValue, Instance, KillResult, LaunchResult, RamCheck } from '../types';

/**
 * Tauri 1 convierte los argumentos snake_case de Rust a camelCase en JS
 * (v1.tauri.app/v1/guides/features/command/#passing-arguments). Todas las
 * claves de los objetos de argumentos de este archivo DEBEN ser camelCase.
 * Las respuestas, en cambio, llegan con los nombres de campo de serde.
 */
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
  minecraftVersion: string;
  fabricLoader: string | null;
}): Promise<Instance> {
  return invoke('create_instance', values);
}

export async function getInstance(id: string): Promise<Instance> {
  return invoke('get_instance', { id });
}

export async function resolveConfig(instanceId: string, key: ConfigKey): Promise<ConfigValue> {
  return invoke('resolve_config', { instanceId, key });
}

export async function setGlobalConfig(
  key: ConfigKey,
  value: ConfigValue,
  force = false,
): Promise<RamCheck> {
  return invoke('set_global_config', { key, value, force });
}

export async function setInstanceConfig(
  instanceId: string,
  key: ConfigKey,
  value: ConfigValue,
  force = false,
): Promise<RamCheck> {
  return invoke('set_instance_config', { instanceId, key, value, force });
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
