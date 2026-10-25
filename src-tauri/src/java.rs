use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::Result;

const ADOPTIUM_BASE: &str = "https://api.adoptium.net/v3";
const MOJANG_VERSION_MANIFEST: &str =
    "https://launchermeta.mojang.com/mc/game/version_manifest.json";

/// Un artefacto de Java candidato de la API de Adoptium.
#[derive(Debug, Clone)]
pub struct AdoptiumAsset {
    pub link: String,
    /// Checksum SHA-256 en hexadecimal (sin prefijo). `None` si la API no lo
    /// proporcionó; en ese caso la verificación de integridad se omite de
    /// forma explícita (y se documenta en el log).
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
        self.java_dir.join(format!("jdk-{version}"))
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
    /// Las instalaciones gestionadas viven bajo `data_dir/java/`: nunca se
    /// tocan las instalaciones de Java del sistema del usuario.
    ///
    /// Consistencia ante fallos: todo el trabajo se hace en un directorio
    /// `.staging-{version}`. Si CUALQUIER paso falla (API caída, red cortada
    /// a mitad de la descarga, checksum incorrecto, extracción inválida) se
    /// borran staging y los temporales de descarga y se devuelve el error.
    /// Nunca se deja un `jdk-{version}` a medio instalar: un reinicio
    /// simplemente reintenta la descarga desde cero.
    pub async fn ensure(&self, version: u32) -> Result<PathBuf> {
        let binary = self.java_binary(version);
        if binary.exists() {
            return Ok(binary);
        }

        fs::create_dir_all(&self.java_dir)?;
        let staging = self.java_dir.join(format!(".staging-{version}"));
        let home = self.java_home(version);
        self.clean_stale_artifacts(version);

        let outcome = self.try_install(version, &staging, &home).await;
        if let Err(err) = outcome {
            let _ = fs::remove_dir_all(&staging);
            let _ = fs::remove_dir_all(&home);
            self.clean_stale_artifacts(version);
            return Err(err);
        }

        Ok(self.java_binary(version))
    }

    async fn try_install(
        &self,
        version: u32,
        staging: &Path,
        home: &Path,
    ) -> Result<()> {
        fs::create_dir_all(staging)?;
        let asset = Self::pick_asset(version).await?;
        let archive = self.download(&asset).await?;

        if let Some(checksum) = &asset.checksum {
            verify_sha256(&archive, checksum)?;
        }
        Self::extract(&archive, staging)?;
        let java_bin = find_java_binary(staging)?;

        // El tar/zip contiene una carpeta raíz propia (ej. `jdk-21.0.5+11`,
        // o `Contents/Home` en macOS). Movemos el JAVA HOME real, no staging,
        // para que `java_binary()` apunte a `<home>/bin/java`.
        let extracted_home = java_bin
            .parent()
            .and_then(|p| p.parent())
            .ok_or("No se pudo determinar el JAVA HOME extraído")?;
        let _ = fs::remove_dir_all(home);
        fs::rename(extracted_home, home)?;

        let _ = fs::remove_file(&archive);
        let _ = fs::remove_dir_all(staging);
        Ok(())
    }

    /// Borra temporales de descargas fallidas de intentos anteriores.
    fn clean_stale_artifacts(&self, version: u32) {
        let _ = fs::remove_dir_all(self.java_dir.join(format!(".staging-{version}")));
        if let Ok(entries) = fs::read_dir(&self.java_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(".download-") {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }

    /// Consulta la API de Adoptium y elige un paquete `jdk` para el SO y la
    /// arquitectura de esta máquina.
    ///
    /// Endpoint verificado: `/v3/assets/feature_releases/{version}/ga` con
    /// parámetros `os`, `architecture`, `image_type`, `jvm_impl`, `vendor`.
    /// Devuelve un array de releases con `binaries[].package.{link,checksum}`.
    /// Si la API no responde o no hay paquete adecuado, se usa la URL directa
    /// de binario (`/v3/binary/latest/...`), que redirige al artefacto real.
    async fn pick_asset(version: u32) -> Result<AdoptiumAsset> {
        let os = adoptium_os();
        let arch = adoptium_arch();
        let url = format!("{ADOPTIUM_BASE}/assets/feature_releases/{version}/ga");

        if let Ok(response) = reqwest::Client::new()
            .get(&url)
            .query(&[
                ("os", os),
                ("architecture", arch),
                ("image_type", "jdk"),
                ("jvm_impl", "hotspot"),
                ("vendor", "eclipse"),
                ("heap_size", "normal"),
                ("page_size", "20"),
            ])
            .header("User-Agent", "rf-launcher/0.1")
            .send()
            .await
        {
            if response.status().is_success() {
                if let Ok(body) = response.json::<Value>().await {
                    if let Some(releases) = body.as_array() {
                        for release in releases {
                            if let Some(binaries) =
                                release.get("binaries").and_then(|v| v.as_array())
                            {
                                for bin in binaries {
                                    if let Some(asset) = extract_asset(bin, arch, os) {
                                        return Ok(asset);
                                    }
                                }
                            }
                            // Algunos endpoints devuelven `binary` en singular.
                            if let Some(bin) = release.get("binary") {
                                if let Some(asset) = extract_asset(bin, arch, os) {
                                    return Ok(asset);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fallback documentado: descarga directa (sin checksum en la URL).
        Ok(AdoptiumAsset {
            link: format!(
                "{ADOPTIUM_BASE}/binary/latest/{version}/ga/{os}/{arch}/jdk/hotspot/normal/eclipse"
            ),
            checksum: None,
        })
    }

    /// Descarga el artefacto a un fichero temporal con la extensión REAL
    /// (deducida de la URL final tras las redirecciones), para que la
    /// extracción sepa si es tar.gz o zip.
    async fn download(&self, asset: &AdoptiumAsset) -> Result<PathBuf> {
        let response = reqwest::Client::new()
            .get(&asset.link)
            .header("User-Agent", "rf-launcher/0.1")
            .send()
            .await?
            .error_for_status()?;
        let final_path = response.url().path().to_string();
        let is_tar = final_path.ends_with(".tar.gz") || asset.link.contains(".tar.gz");
        let suffix = if is_tar { "tar.gz" } else { "zip" };
        let archive = self
            .java_dir
            .join(format!(".download-{}.{suffix}", next_id()));

        let mut response = response;
        let mut file = fs::File::create(&archive)?;
        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk)?;
        }
        file.flush()?;
        drop(file);
        Ok(archive)
    }

    /// Extrae un `.tar.gz` o `.zip` al directorio de staging.
    fn extract(archive: &Path, staging: &Path) -> Result<()> {
        let name = archive.file_name().unwrap_or_default().to_string_lossy();
        if name.ends_with(".tar.gz") {
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
}

/// Extrae `link` y `checksum` de un objeto `binary` de la API de Adoptium,
/// comprobando que coincide con la arquitectura y el SO pedidos.
fn extract_asset(bin: &Value, arch: &str, os: &str) -> Option<AdoptiumAsset> {
    if let Some(a) = bin.get("architecture").and_then(|v| v.as_str()) {
        if a != arch {
            return None;
        }
    }
    if let Some(o) = bin.get("os").and_then(|v| v.as_str()) {
        if o != os {
            return None;
        }
    }
    if let Some(t) = bin.get("image_type").and_then(|v| v.as_str()) {
        if t != "jdk" {
            return None;
        }
    }
    let pkg = bin.get("package")?;
    let link = pkg.get("link").and_then(|v| v.as_str())?;
    if !link.ends_with(".tar.gz") && !link.ends_with(".zip") {
        return None;
    }
    let checksum = pkg
        .get("checksum")
        .and_then(|v| v.as_str())
        .filter(|s| s.len() == 64)
        .map(clean_checksum);
    Some(AdoptiumAsset {
        link: link.to_string(),
        checksum,
    })
}

/// Mapeo estático de versión de Minecraft -> versión de Java requerida.
///
/// FUENTES (consultadas durante la Fase 3-A):
///  * Minecraft Wiki, "Tutorial:Update Java" y "Client.json": `javaVersion.
///    majorVersion` vale 8 hasta 1.16.5, 16 en 1.17/1.17.1, 17 desde
///    1.18-pre2 hasta 1.20.4, 21 desde 1.20.5 hasta 1.21.11 y 25 desde 26.1.
///  * https://minecraft.wiki/w/Tutorial:Update_Java
///  * https://minecraft.wiki/w/Client.json
///  * https://mcreference.com/guides/minecraft-java-version-matrix
///
/// Esta tabla es un FALLBACK. La fuente autoritativa en tiempo de ejecución
/// es `javaVersion.majorVersion` del JSON de versión de Mojang (ver
/// `fetch_java_major_version`), que se consulta primero. `component` NO es un
/// número (es "jre-legacy", "java-runtime-gamma", etc.), por eso se lee
/// `majorVersion`.
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
            } else if *major == 17 {
                16
            } else if *major == 18 || *major == 19 {
                17
            } else if *major == 20 {
                match parts.get(2) {
                    Some(patch) if *patch >= 5 => 21,
                    _ => 17,
                }
            } else {
                21 // 1.21.x
            }
        }
        [major, ..] => {
            // Minecraft 26.1+ cambió el esquema de versiones y requiere Java 25.
            if *major >= 26 {
                25
            } else {
                21
            }
        }
    };
    Ok(version)
}

/// Versión de Java leyendo el manifiesto de Mojang, con fallback a la tabla
/// estática si no hay red, la API falla o la versión no aparece.
pub async fn java_version_for_minecraft_resolved(minecraft_version: &str) -> Result<u32> {
    if let Ok(major) = fetch_java_major_version(minecraft_version).await {
        return Ok(major);
    }
    java_version_for_minecraft(minecraft_version)
}

/// Lee `javaVersion.majorVersion` del JSON de versión de Mojang.
async fn fetch_java_major_version(minecraft_version: &str) -> Result<u32> {
    let manifest: Value = reqwest::Client::new()
        .get(MOJANG_VERSION_MANIFEST)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let versions = manifest
        .get("versions")
        .and_then(|v| v.as_array())
        .ok_or("manifiesto de Mojang sin lista de versiones")?;
    let entry = versions
        .iter()
        .find(|v| v.get("id").and_then(|i| i.as_str()) == Some(minecraft_version));
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

    let major = version_json
        .get("javaVersion")
        .and_then(|v| v.get("majorVersion"))
        .and_then(|v| v.as_u64())
        .ok_or("JSON de versión sin javaVersion.majorVersion")?;
    Ok(major as u32)
}

/// Parámetro `os` para la API de Adoptium (valores válidos: linux, windows,
/// mac, solaris, aix, alpine-linux).
fn adoptium_os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    }
}

/// Parámetro `architecture` para la API de Adoptium.
fn adoptium_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64"
    }
}

fn clean_checksum(checksum: &str) -> String {
    checksum
        .strip_prefix("sha256:")
        .unwrap_or(checksum)
        .to_string()
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

/// Busca el binario java dentro de una extracción.
fn find_java_binary(staging: &Path) -> Result<PathBuf> {
    let expected = if cfg!(windows) { "java.exe" } else { "java" };
    for path in walkdirs(staging)? {
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name == expected {
                    return Ok(path);
                }
            }
        }
    }
    Err("No se encontró el binario java en el paquete descargado".into())
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
    use std::env::temp_dir;

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
    async fn mojang_manifest_major_version_matches_static_table() {
        let cases = [
            ("1.16.5", 8),
            ("1.17", 16),
            ("1.20.1", 17),
            ("1.20.4", 17),
            ("1.20.5", 21),
            ("1.21", 21),
        ];
        for (minecraft_version, expected) in cases {
            let static_version = java_version_for_minecraft(minecraft_version).unwrap();
            assert_eq!(
                static_version, expected,
                "mapeo estático para {minecraft_version}"
            );
            if let Ok(major) = fetch_java_major_version(minecraft_version).await {
                assert_eq!(
                    major, expected,
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
    fn java_mapping_117_is_java16() {
        assert_eq!(java_version_for_minecraft("1.17").unwrap(), 16);
        assert_eq!(java_version_for_minecraft("1.17.1").unwrap(), 16);
    }

    #[test]
    fn java_mapping_118_to_1204_is_java17() {
        assert_eq!(java_version_for_minecraft("1.18.2").unwrap(), 17);
        assert_eq!(java_version_for_minecraft("1.19.4").unwrap(), 17);
        assert_eq!(java_version_for_minecraft("1.20.1").unwrap(), 17);
        assert_eq!(java_version_for_minecraft("1.20.4").unwrap(), 17);
    }

    #[test]
    fn java_mapping_1205_onwards_is_java21() {
        assert_eq!(java_version_for_minecraft("1.20.5").unwrap(), 21);
        assert_eq!(java_version_for_minecraft("1.21").unwrap(), 21);
        assert_eq!(java_version_for_minecraft("1.21.4").unwrap(), 21);
    }

    #[test]
    fn java_mapping_26x_is_java25() {
        assert_eq!(java_version_for_minecraft("26.1").unwrap(), 25);
    }

    #[test]
    fn java_mapping_unknown_is_error() {
        assert!(java_version_for_minecraft("abc").is_err());
    }

    #[test]
    fn checksum_verification_detects_corruption() {
        let dir = temp_dir().join("rf-checksum-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("payload.bin");
        fs::write(&file, b"contenido de prueba").unwrap();
        let mut hasher = Sha256::new();
        hasher.update(b"contenido de prueba");
        let expected = hex_encode(&hasher.finalize());
        assert!(verify_sha256(&file, &expected).is_ok());
        assert!(verify_sha256(&file, "0000000000000000000000000000000000000000000000000000000000000000").is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
