export type ConfigValue =
  | { kind: 'Int'; value: number }
  | { kind: 'Bool'; value: boolean }
  | { kind: 'String'; value: string };

export type ConfigKey = 'RamMinMb' | 'RamMaxMb' | 'JavaAutoDownload' | 'Theme';

/**
 * Réplica EXACTA del cable JSON que serializa Rust.
 *
 * CONVENCIÓN IPC (issue #6): TODAS las respuestas Rust -> TypeScript usan
 * camelCase, incluidos los campos que se persisten en `instance.json`. Los
 * structs Rust llevan `#[serde(rename_all = "camelCase")]` y conservan
 * `#[serde(alias = "...")]` para leer ficheros legacy en snake_case. Los
 * argumentos TS -> Rust también son camelCase: Tauri 2 convierte los
 * parámetros snake_case de Rust a camelCase en JS
 * (v2.tauri.app/develop/calling-rust/#passing-arguments). Los nombres de los
 * comandos (`invoke('list_instances')`, etc.) siguen siendo snake_case.
 */
export interface Instance {
  id: string;
  name: string;
  minecraftVersion: string;
  fabricLoader: string | null;
  templateOrigin: string | null;
  configOverrides: Record<string, ConfigValue>;
  ramMinMb: number | null;
  ramMaxMb: number | null;
  createdAt: number;
  lastLaunchedAt: number | null;
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
  totalMb: number;
  ok: boolean;
  warning?: string;
}

/** Evento real emitido por Rust durante la descarga de Java. */
export interface JavaProgressEvent {
  version: number;
  stage: string;
  downloadedBytes: number;
  totalBytes: number | null;
  percent: number | null;
  message: string;
}

export interface CatalogEnvelope {
  sourceUrl: string;
  fetchedAt: string;
  sha256: string;
  payload: unknown;
}

export interface CatalogVersion {
  id: string;
  versionType: string;
  url: string;
}

export interface FabricLoaderInfo {
  version: string;
  stable: boolean;
  maven: string;
  mainClass: string;
}
