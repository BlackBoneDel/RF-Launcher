//! Catálogo de versiones: caché con TTL y fallback a fixtures versionados.
//!
//! En esta sub-ronda NO se exponen selectores de UI (Módulos I/J). Este módulo
//! deja lista la capa de datos aprobada en la Sección 0: manifiesto v2 de
//! Mojang y metadatos de Fabric, con refresco periódico y degradación sin red.

#![allow(dead_code)] // La UI de selectores llega en la sub-ronda 2.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const MOJANG_MANIFEST_V2_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const FABRIC_VERSIONS_URL: &str = "https://meta.fabricmc.net/v2/versions";

const MOJANG_FIXTURE: &str = include_str!("../../catalog-fixtures/mojang-version-manifest-v2.json");
const FABRIC_FIXTURE: &str = include_str!("../../catalog-fixtures/fabric-loader-1.21.1.json");

pub fn fabric_loader_url(game_version: &str) -> String {
    format!("https://meta.fabricmc.net/v2/versions/loader/{game_version}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEnvelope {
    #[serde(alias = "source_url")]
    pub source_url: String,
    #[serde(alias = "fetched_at")]
    pub fetched_at: String,
    pub sha256: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogVersion {
    pub id: String,
    #[serde(alias = "version_type")]
    pub version_type: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricLoaderInfo {
    pub version: String,
    pub stable: bool,
    pub maven: String,
    #[serde(alias = "main_class")]
    pub main_class: String,
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Verifica el sobre del fixture y devuelve su payload ya parseado.
///
/// El hash se calcula sobre `serde_json::to_vec(payload)`, que coincide con el
/// canonical JSON (`sort_keys`, sin espacios) usado por
/// `scripts/verify_catalog_fixtures.py`.
pub fn verify_fixture(raw: &str) -> Result<Value, String> {
    let envelope: CatalogEnvelope =
        serde_json::from_str(raw).map_err(|e| format!("fixture inválido: {e}"))?;
    if envelope.source_url.is_empty()
        || envelope.fetched_at.is_empty()
        || envelope.sha256.len() != 64
    {
        return Err("sobre de fixture incompleto".into());
    }
    let canonical = serde_json::to_vec(&envelope.payload)
        .map_err(|e| format!("no se pudo canonicalizar payload: {e}"))?;
    let digest = Sha256::digest(&canonical);
    let actual = hex(digest.as_ref());
    if actual != envelope.sha256 {
        return Err(format!(
            "SHA-256 de fixture no coincide: esperado {}, obtenido {}",
            envelope.sha256, actual
        ));
    }
    Ok(envelope.payload)
}

pub fn mojang_fixture() -> Result<Value, String> {
    verify_fixture(MOJANG_FIXTURE)
}

pub fn fabric_fixture() -> Result<Value, String> {
    verify_fixture(FABRIC_FIXTURE)
}

pub fn parse_mojang_versions(payload: &Value) -> Result<Vec<CatalogVersion>, String> {
    let versions = payload
        .get("versions")
        .and_then(|v| v.as_array())
        .ok_or("manifiesto sin versions")?;
    let mut out = Vec::new();
    for entry in versions {
        let id = entry
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("versión sin id")?;
        let version_type = entry
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("release");
        let url = entry
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or("versión sin url")?;
        out.push(CatalogVersion {
            id: id.to_string(),
            version_type: version_type.to_string(),
            url: url.to_string(),
        });
    }
    Ok(out)
}

pub fn parse_fabric_loaders(payload: &Value) -> Result<Vec<FabricLoaderInfo>, String> {
    let entries = payload
        .as_array()
        .ok_or("respuesta de Fabric no es un array")?;
    let mut out = Vec::new();
    for entry in entries {
        let loader = entry.get("loader").ok_or("entrada sin loader")?;
        let version = loader
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or("loader sin version")?;
        let stable = loader
            .get("stable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let maven = loader
            .get("maven")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let main_class = entry
            .get("launcherMeta")
            .and_then(|m| m.get("mainClass"))
            .and_then(|m| m.get("client"))
            .and_then(|v| v.as_str())
            .unwrap_or("net.fabricmc.loader.impl.launch.knot.KnotClient");
        out.push(FabricLoaderInfo {
            version: version.to_string(),
            stable,
            maven: maven.to_string(),
            main_class: main_class.to_string(),
        });
    }
    Ok(out)
}

/// Caché local con TTL. Si el fichero está fresco se usa; si no, se intenta
/// refrescar por red. Si la red falla SIEMPRE se devuelve el payload del
/// fixture, nunca un error que bloquee la creación de instancias.
pub struct CatalogCache {
    pub dir: PathBuf,
    pub ttl: Duration,
}

impl Default for CatalogCache {
    fn default() -> Self {
        Self {
            dir: std::env::temp_dir().join("rf-launcher-catalog"),
            ttl: Duration::from_secs(24 * 60 * 60),
        }
    }
}

impl CatalogCache {
    pub fn new(dir: PathBuf, ttl: Duration) -> Self {
        Self { dir, ttl }
    }

    fn path_for(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.json"))
    }

    fn read_fresh(&self, key: &str) -> Option<Value> {
        let path = self.path_for(key);
        let modified = fs::metadata(&path).ok()?.modified().ok()?;
        let age = SystemTime::now().duration_since(modified).ok()?;
        if age > self.ttl {
            return None;
        }
        let text = fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn write(&self, key: &str, value: &Value) -> Result<(), String> {
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
        fs::write(self.path_for(key), text).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn get(
        &self,
        key: &str,
        url: &str,
        fixture_raw: &str,
    ) -> Result<Value, String> {
        let fixture = verify_fixture(fixture_raw)?;
        if let Some(cached) = self.read_fresh(key) {
            return Ok(cached);
        }
        match reqwest::Client::new()
            .get(url)
            .timeout(Duration::from_secs(20))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                if let Ok(value) = response.json::<Value>().await {
                    let _ = self.write(key, &value);
                    return Ok(value);
                }
            }
            _ => {}
        }
        Ok(fixture)
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_integrity_and_parse() {
        let mojang = mojang_fixture().unwrap();
        let versions = parse_mojang_versions(&mojang).unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].id, "26.4-snapshot-6");
        assert_eq!(versions[1].id, "26.3");

        let fabric = fabric_fixture().unwrap();
        let loaders = parse_fabric_loaders(&fabric).unwrap();
        assert_eq!(loaders.len(), 2);
        assert_eq!(loaders[0].version, "0.20.1");
        assert!(loaders[0].stable);
        assert!(!loaders[1].stable);
    }

    #[test]
    fn tampered_fixture_is_rejected() {
        let raw = MOJANG_FIXTURE.replace("\"26.3\"", "\"26.4\"");
        assert!(verify_fixture(&raw).is_err());
    }

    #[test]
    fn stale_cache_is_not_reused() {
        let dir = std::env::temp_dir().join(format!("rf-catalog-test-{}", now_secs()));
        let _ = fs::remove_dir_all(&dir);
        let cache = CatalogCache::new(dir.clone(), Duration::from_secs(0));
        let value = serde_json::json!({"ok": true});
        cache.write("x", &value).unwrap();
        assert!(cache.read_fresh("x").is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
