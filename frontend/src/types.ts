export type ConfigValue =
  | { kind: 'Int'; value: number }
  | { kind: 'Bool'; value: boolean }
  | { kind: 'String'; value: string };

export type ConfigKey = 'RamMinMb' | 'RamMaxMb' | 'JavaAutoDownload' | 'Theme';

export interface Instance {
  id: string;
  name: string;
  minecraft_version: string;
  fabric_loader: string | null;
  template_origin: string | null;
  data_dir: string | null;
  ram_min_mb: number | null;
  ram_max_mb: number | null;
  created_at: number;
  last_launched_at: number | null;
}

export interface LaunchResult {
  ok: boolean;
  pid?: number;
  error?: string;
}

export interface KillResult {
  ok: boolean;
  terminated: boolean;
  error?: string;
}

export interface RamCheck {
  total_mb: number;
  ok: boolean;
  warning?: string;
}
