export type ConfigValue =
  | { kind: 'Int'; value: number }
  | { kind: 'Bool'; value: boolean }
  | { kind: 'String'; value: string };

export type ConfigKey = 'RamMinMb' | 'RamMaxMb' | 'JavaAutoDownload' | 'Theme';

/**
 * Réplica EXACTA del `Instance` que serializa Rust (src-tauri/src/instance.rs).
 *
 * Convención del cable, deliberadamente mixta y documentada:
 *  - RESPUESTAS (Rust -> TS): serde usa los nombres de los campos Rust.
 *    Casi todos son snake_case; `templateOrigin` es camelCase porque el plan
 *    de Fase 3-A lo fijó así y el mod lo consumirá como plantilla.
 *  - ARGUMENTOS (TS -> Rust): Tauri 1 convierte los parámetros snake_case de
 *    Rust a camelCase en JS. Por eso en `lib/tauri.ts` se usa `instanceId`,
 *    `minecraftVersion`, `fabricLoader`. Ver Parte F del informe de auditoría.
 */
export interface Instance {
  id: string;
  name: string;
  minecraft_version: string;
  fabric_loader: string | null;
  templateOrigin: string | null;
  config_overrides: Record<string, ConfigValue>;
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
