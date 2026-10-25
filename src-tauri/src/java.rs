use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::Result;

const ADOPTIUM_BASE: &str = "https://api.adoptium.net/v3/assets";

/// Un artefacto de Java candidato de la API de Adoptium.
#[derive(Debug, Clone)]
pub struct AdoptiumAsset {
    pub link: String,
    /// Checksum SHA-256, con o sin prefijo "sha256:".
    pub checksum: Option<String>,
}

pub struct JavaManager {
    java_dir: PathBuf,
}

impl JavaManager {
    pub fn new(java_dir: PathBuf) -> Self {
        Self { java_dir }
    }

    /// Carpeta donde se instala un JDK concreto gestionado por el launcher.
    pub fn java_home(&self, version: u32) -> PathBuf {
        self.java_dir.join(format!("jdk-{}", version))
    }

    /// Binario java de una instalación gestionada.
    pub fn java_binary(&self, version: u32) -> PathBuf {
        if cfg!(windows) {
            self.java_home(version).join("bin").join("java.exe")
        } else {
            self.java_home(version).join("bin").join("java")
        }
    }

    /// Devuelve la ruta del java ya instalado o lo descarga de Adoptium.
    ///
    /// Las instalaciones gestionadas por el launcher viven bajo
    /// data_dir/java/, nunca se tocan instalaciones de Java del usuario.
    pub async fn ensure(&self, version: u32) -> Result<PathBuf> {
        let binary = self.java_binary(version);
        if binary.exists() {
            return Ok(binary);
        }

        fs::create_dir_all(&self.java_dir)?;
        let staging = self.java_dir.join(format!(".staging-{version}"));
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(&staging)?;

        let asset = self.pick_asset(version).await?;
        let tmp = self.download(&asset).await?;
        if let Some(checksum) = &asset.checksum {
            verify_sha256(&tmp, checksum)?;
        }
        extract(&tmp, &staging)?;
        let java_bin = find_java_binary(&staging)?;
        let _ = java_bin; // validate que existe antes de renombrar
        fs::rename(&staging, self.java_home(version))?;
        Ok(self.java_binary(version))
    }

    /// Consulta la API de Adoptium y elige el primer paquete (tar.gz o zip)
    /// para el SO y arquitectura de esta máquina.
    async fn pick_asset(&self, version: u32) -> Result<AdoptiumAsset> {
        let os = adoptium_os();
        let arch = adoptium_arch();

        let url = format!("{ADOPTIUM_BASE}/version/{version}/latest");
        let assets: Vec<Value> = reqwest::Client::new()
            .get(&url)
            .query(&[
                ("os", os),
                ("arch", arch),
                ("image_type", "jdk"),
                ("release", "ga"),
                ("vendor", "eclipse"),
                ("heap_size", "normal"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        for item in assets {
            let binaries = match item.get("binaries").and_then(|v| v.as_array()) {
                Some(b) => b,
                None => continue,
            };
            for bin in binaries {
                let pkg = match bin.get("package") {
                    Some(p) => p,
                    None => continue,
                };
                let link = match pkg.get("link").and_then(|v| v.as_str()) {
                    Some(l) => l,
                    None => continue,
                };
                if !link.ends_with(".tar.gz") && !link.ends_with(".zip") {
                    continue;
                }
                let checksum = pkg
                    .get("checksum")
                    .and_then(|v| v.as_str())
                    .map(clean_checksum);
                return Ok(AdoptiumAsset {
                    link: link.to_string(),
                    checksum,
                });
            }
        }

        Err(format!(
            "Adoptium no tiene un paquete de Java {version} para {os}/{arch}"
        )
        .into())
    }

    async fn download(&self, asset: &AdoptiumAsset) -> Result<PathBuf> {
        let tmp = self.java_dir.join(format!(".download-{}.tmp", next_id()));
        let mut response = reqwest::Client::new().get(&asset.link).send().await?.error_for_status()?;
        let mut file = fs::File::create(&tmp)?;
        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk)?;
        }
        drop(file);
        Ok(tmp)
    }

    /// Extrae un .tar.gz o .zip al directorio de staging.
    fn extract(&self, archive: &Path, staging: &Path) -> Result<()> {
        if archive.extension().and_then(|e| e.to_str()) == Some("gz") {
            let file = fs::File::open(archive)?;
            let gz = flate2::read::GzDecoder::new(file);
            let mut tar = tar::Archive::new(gz);
            tar.unpack(staging)?;
        } else {
            let file = fs::File::open(archive)?;
            let mut zip = zip::ZipArchive::new(file)?;
            zip.extract(staging)?;
        }
        Ok(())
    }

    /// Busca el binario java dentro de una extracción (la carpeta raíz puede
    /// llamarse jdk-21.0.x_7, etc.).
    fn find_java_binary(staging: &Path) -> Result<PathBuf> {
        let expected = if cfg!(windows) { "java.exe" } else { "java" };
        for entry in walkdirs(staging)? {
            if entry.is_file() {
                if let Some(name) = entry.file_name().to_str() {
                    if name == expected {
                        return Ok(entry.path().to_path_buf());
                    }
                }
            }
        }
        Err("No se encontró el binario java en el paquete descargado".into())
    }
}

/// Mapeo estático de versión de Minecraft → versión de Java requerida.
///
/// Fuentes (manifiesto de versiones de Mojang / documentación de Minecraft):
///   - <= 1.16.5        -> Java 8
///   - 1.17 - 1.20.4    -> Java 16
///   - 1.20.5 en adelante -> Java 21
///
/// Este es un fallback: en tiempo de ejecución se prefiere leer
/// `javaVersion.component` del JSON de versión de Mojang
/// (launchermeta.mojang.com), que es la fuente autoritativa. Si no hay red o
/// la versión no aparece, se usa esta tabla.
pub fn java_version_for_minecraft(minecraft_version: &str) -> Result<u32> {
    let parts: Vec<u32> = minecraft_version
        .split('.')
        .filter_map(|p| p.trim().parse::<u32>().ok())
        .collect();

    let version = match parts.as_slice() {
        [] | [_] => {
            return Err(format!(
                "Versión de Minecraft no reconocida: {minecraft_version}"
            )
            .into())
        }
        [1, major, ..] => {
            if *major <= 16 {
                8
            } else if *major == 20 {
                // 1.20.5+ requiere Java 21; 1.20.0-1.20.4 requieren 16
                match parts.get(2) {
                    Some(patch) if *patch >= 5 => 21,
                    _ => 16,
                }
            } else if (17..=19).contains(major) {
                16
            } else {
                21
            }
        }
        [major, ..] if *major >= 2 => 21, // versiones futuras: asume moderno
    };
    Ok(version)
}

/// Versión de Java leyendo el manifiesto de Mojang, con fallback a la tabla
/// estática si no hay red o la versión no aparece.
pub async fn java_version_for_minecraft_resolved(minecraft_version: &str) -> Result<u32> {
    if let Ok(component) = fetch_java_component(minecraft_version).await {
        return Ok(component);
    }
    java_version_for_minecraft(minecraft_version)
}

/// Lee javaVersion.component del JSON de versión de Mojang.
async fn fetch_java_component(minecraft_version: &str) -> Result<u32> {
    let manifest: Value = reqwest::Client::new()
        .get("https://launchermeta.mojang.com/mc/game/version_manifest.json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let versions = manifest
        .get("versions")
        .and_then(|v| v.as_array())
        .ok_or("manifiesto sin lista de versiones")?;
    let entry = versions.iter().find(|v| {
        v.get("id").and_then(|i| i.as_str()) == Some(minecraft_version)
    });
    let Some(entry) = entry else {
        return Err(format!(
            "versión {minecraft_version} no está en el manifiesto de Mojang"
        )
        .into());
    };
    let version_url = entry
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or("entrada del manifiesto sin url")?;

    let version_json: Value = reqwest::Client::new()
        .get(version_url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let component = version_json
        .get("javaVersion")
        .and_then(|v| v.get("component"))
        .and_then(|v| v.as_u64())
        .ok_or("JSON de versión sin javaVersion.component")?;
    Ok(component as u32)
}

/// Parámetro `os` para la API de Adoptium.
fn adoptium_os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac-os-x"
    } else {
        "linux"
    }
}

/// Parámetro `arch` para la API de Adoptium.
fn adoptium_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64"
    }
}

fn clean_checksum(checksum: &str) -> String {
    checksum.strip_prefix("sha256:").unwrap_or(checksum).to_string()
}

fn verify_sha256(file: &Path, expected: &str) -> Result<()> {
    let mut f = fs::File::open(file)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let actual = hex_encode(&digest);
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(format!(
            "Checksum SHA-256 no coincide: esperado {expected}, obtenido {actual}"
        )
        .into())
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

static DOWNLOAD_COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_id() -> String {
    let n = DOWNLOAD_COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{now}-{n:x}")
}

/// Walk simple de directorio (sin dependencia extra).
fn walkdirs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn ensure_java_21_downloads_from_adoptium() {
        let dir = temp_dir().join(format!("rf-java-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mgr = JavaManager::new(dir.clone());
        let bin = mgr
            .ensure(21)
            .await
            .expect("Java 21 debe descargarse de Adoptium y extraerse");
        assert!(bin.exists(), "el binario java debe existir tras la descarga");
        let output = std::process::Command::new(&bin)
            .arg("-version")
            .output()
            .expect("java -version debe ejecutarse");
        assert!(
            output.status.success(),
            "java -version falló: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn mojang_manifest_java_component_matches_static_table() {
        // Verifica que el mapeo estático coincida con la fuente autoritativa
        // (javaVersion.component del manifiesto de Mojang) cuando hay red.
        let cases = [("1.16.5", 8), ("1.20.1", 16), ("1.20.4", 16), ("1.21", 21)];
        for (minecraft_version, expected) in cases {
            let static_version = java_version_for_minecraft(minecraft_version).unwrap();
            assert_eq!(static_version, expected, "mapeo estático para {minecraft_version}");
            if let Ok(component) = fetch_java_component(minecraft_version).await {
                assert_eq!(
                    component,
                    expected,
                    "manifiesto de Mojang para {minecraft_version}"
                );
            }
        }
    }

    #[test]
    fn java_mapping_legacy_is_java8() {
        assert_eq!(java_version_for_minecraft("1.16.5").unwrap(), 8);
        assert_eq!(java_version_for_minecraft("1.12.2").unwrap(), 8);
        assert_eq!(java_version_for_minecraft("1.8.9").unwrap(), 8);
    }

    #[test]
    fn java_mapping_modern_is_java16() {
        assert_eq!(java_version_for_minecraft("1.17").unwrap(), 16);
        assert_eq!(java_version_for_minecraft("1.18.2").unwrap(), 16);
        assert_eq!(java_version_for_minecraft("1.19.4").unwrap(), 16);
        assert_eq!(java_version_for_minecraft("1.20.1").unwrap(), 16);
        assert_eq!(java_version_for_minecraft("1.20.4").unwrap(), 16);
    }

    #[test]
    fn java_mapping_newer_is_java21() {
        assert_eq!(java_version_for_minecraft("1.20.5").unwrap(), 21);
        assert_eq!(java_version_for_minecraft("1.21").unwrap(), 21);
        assert_eq!(java_version_for_minecraft("1.21.4").unwrap(), 21);
    }

    #[test]
    fn java_mapping_unknown_is_error() {
        assert!(java_version_for_minecraft("9.9.9").is_err());
    }
}
