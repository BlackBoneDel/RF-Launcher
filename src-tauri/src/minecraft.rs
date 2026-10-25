use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use sha1::{Digest, Sha1};

use crate::error::{LauncherError, Result};

const MOJANG_VERSION_MANIFEST: &str =
    "https://launchermeta.mojang.com/mc/game/version_manifest.json";
const FABRIC_MAVEN: &str = "https://maven.fabricmc.net/net/fabricmc/fabric-loader";

/// Datos resueltos para lanzar una versión de Minecraft (vanilla o con Fabric).
pub struct ResolvedLaunch {
    pub main_class: String,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
    pub classpath: Vec<PathBuf>,
    pub working_dir: PathBuf,
    pub log_file: PathBuf,
}

/// Contexto de sustitución de los placeholders de los argumentos de lanzamiento.
struct LaunchContext {
    version_name: String,
    version_type: String,
    game_directory: PathBuf,
    assets_root: PathBuf,
    assets_index_name: String,
    auth_player_name: String,
    auth_uuid: String,
    auth_access_token: String,
    user_type: String,
    user_properties: String,
    resolution_width: String,
    resolution_height: String,
    library_directory: PathBuf,
    classpath_value: String,
    natives_directory: PathBuf,
}

/// Descarga (si falta) y resuelve todo lo necesario para lanzar: manifiesto de
/// Mojang, JSON de versión, librerías, jar del cliente y (opcional) Fabric
/// Loader. Todo se guarda bajo data_dir/assets/.
pub async fn resolve(
    assets_dir: &Path,
    instances_dir: &Path,
    instance_id: &uuid::Uuid,
    minecraft_version: &str,
    fabric_loader: Option<&str>,
) -> Result<ResolvedLaunch> {
    fs::create_dir_all(assets_dir)?;
    let version_dir = assets_dir.join("versions").join(minecraft_version);
    fs::create_dir_all(&version_dir)?;

    // 1. Manifiesto de Mojang -> URL del JSON de esta versión.
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
        .ok_or("manifiesto de Mojang sin versiones")?;
    let entry = versions
        .iter()
        .find(|v| v.get("id").and_then(|i| i.as_str()) == Some(minecraft_version));
    let Some(entry) = entry else {
        return Err(LauncherError::Custom(format!(
            "versión de Minecraft no encontrada en el manifiesto: {minecraft_version}"
        )));
    };
    let version_url = entry
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or("entrada de versión sin url")?;
    let version_sha1 = entry.get("sha1").and_then(|v| v.as_str());

    // 2. JSON de la versión.
    let version_json_path = version_dir.join(format!("{minecraft_version}.json"));
    if !version_json_path.exists() {
        download_file(version_url, &version_json_path, version_sha1).await?;
    }
    let version_json_text = fs::read_to_string(&version_json_path)?;
    let version_json: Value = serde_json::from_str(&version_json_text)?;

    let main_class = version_json
        .get("main_class")
        .and_then(|v| v.as_str())
        .ok_or("version_json sin main_class")?
        .to_string();

    // 3. Librerías: filtrar por reglas de SO, descargar artefactos, construir classpath.
    let lib_dir = assets_dir.join("libraries");
    let natives_dir = version_dir.join("natives");
    fs::create_dir_all(&natives_dir)?;
    let mut classpath: Vec<PathBuf> = Vec::new();

    let libraries = version_json
        .get("libraries")
        .and_then(|v| v.as_array())
        .ok_or("version_json sin libraries")?;
    for lib in libraries {
        if !rules_apply(lib) {
            continue;
        }
        let name = lib.get("name").and_then(|v| v.as_str());
        let downloads = lib.get("downloads");

        // Artefacto principal
        if let Some(artifact) = downloads.and_then(|d| d.get("artifact")) {
            if let Some(url) = artifact.get("url").and_then(|v| v.as_str()) {
                let path = lib
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or("librería sin path")?;
                let dest = lib_dir.join(path);
                let sha1 = artifact.get("sha1").and_then(|v| v.as_str());
                download_file(url, &dest, sha1).await?;
                classpath.push(dest);
            }
        }

        // Nativos (lwjgl, etc.) para el SO actual
        if let Some(classifiers) = downloads.and_then(|d| d.get("classifiers")) {
            let classifier = native_classifier();
            if let Some(native) = classifiers.get(classifier) {
                if let Some(url) = native.get("url").and_then(|v| v.as_str()) {
                    let dest = version_dir.join(format!("natives-{classifier}.jar"));
                    let sha1 = native.get("sha1").and_then(|v| v.as_str());
                    download_file(url, &dest, sha1).await?;
                    extract_natives(&dest, &natives_dir)?;
                    classpath.push(dest);
                }
            }
        }
    }

    // 4. Jar del cliente de Minecraft.
    let client = version_json
        .get("downloads")
        .and_then(|d| d.get("client"))
        .ok_or("version_json sin downloads.client")?;
    let client_url = client
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or("downloads.client sin url")?;
    let client_sha1 = client.get("sha1").and_then(|v| v.as_str());
    let client_jar = version_dir.join(format!("{minecraft_version}.jar"));
    download_file(client_url, &client_jar, client_sha1).await?;
    classpath.push(client_jar.clone());

    // 5. Fabric Loader: su jar va al frente del classpath y su main class
    //    reemplaza a la de vanilla, recibiendo la original por propiedad de sistema.
    let (main_class, mut jvm_args) = if let Some(loader_version) = fabric_loader {
        let loader_jar = assets_dir
            .join("fabric-loader")
            .join(format!("fabric-loader-{loader_version}.jar"));
        fs::create_dir_all(loader_jar.parent().unwrap())?;
        if !loader_jar.exists() {
            let loader_url = format!("{FABRIC_MAVEN}/{loader_version}/fabric-loader-{loader_version}.jar");
            download_file(&loader_url, &loader_jar, None).await?;
        }
        classpath.insert(0, loader_jar);
        jvm_args.push(format!("-Dfabric.mainClass={main_class}"));
        ("net.fabricmc.loader.impl.launch.knot.KnotClient".to_string(), jvm_args)
    } else {
        (main_class, Vec::new())
    };

    // 7. Contexto de sustitución de placeholders (se construye antes de resolver
    //    los argumentos JVM y del juego).
    let game_dir = instances_dir.join(instance_id.to_string()).join("minecraft");
    let assets_index = version_json
        .get("assetIndex")
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("1.20")
        .to_string();
    // El índice debe vivir en <assets_root>/indexes/<id>.json (los objetos
    // los descarga el propio cliente bajo demanda).
    if let Some(ai) = version_json.get("assetIndex").and_then(|v| v.get("url")) {
        if let Some(url) = ai.as_str() {
            let _ = download_file(
                url,
                &assets_dir.join("indexes").join(format!("{assets_index}.json")),
                None,
            )
            .await;
        }
    }

    let ctx = LaunchContext {
        version_name: minecraft_version.to_string(),
        version_type: entry
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("release")
            .to_string(),
        game_directory: game_dir.clone(),
        assets_root: assets_dir.to_path_buf(),
        assets_index_name: assets_index,
        auth_player_name: "Player".to_string(),
        auth_uuid: "00000000-0000-0000-0000-000000000000".to_string(),
        auth_access_token: String::new(),
        user_type: "mojang".to_string(),
        user_properties: "{}".to_string(),
        resolution_width: "854".to_string(),
        resolution_height: "480".to_string(),
        library_directory: lib_dir,
        classpath_value: classpath_to_string(&classpath),
        natives_directory: natives_dir,
    };

    // 6. Argumentos JVM: quitar flags de memoria existentes (nosotros los ponemos al lanzar).
    //    Se resuelven con el contexto real para que ${natives_directory} etc. se sustituyan.
    let mut jvm_args_all: Vec<String> = version_json
        .get("arguments")
        .and_then(|a| a.get("jvm"))
        .and_then(|v| v.as_array())
        .map(|arr| resolve_json_args(arr, &ctx))
        .unwrap_or_default();
    jvm_args_all.retain(|a| !(a.starts_with("-Xms") || a.starts_with("-Xmx")));
    jvm_args_all.extend(jvm_args);

    fs::create_dir_all(&game_dir)?;
    // Versiones >= 1.13 usan `arguments.game` (array). Las anteriores usan
    // `minecraftArguments` (string con placeholders separados por espacios).
    let game_args: Vec<String> = match version_json
        .get("arguments")
        .and_then(|a| a.get("game"))
        .and_then(|v| v.as_array())
    {
        Some(arr) => resolve_json_args(arr, &ctx),
        None => version_json
            .get("minecraftArguments")
            .and_then(|v| v.as_str())
            .map(|s| {
                s.split_whitespace()
                    .map(|arg| substitute(arg, &ctx))
                    .collect()
            })
            .unwrap_or_default(),
    };

    let log_file = instances_dir
        .join(instance_id.to_string())
        .join("logs")
        .join(format!("launch-{}.log", now_secs()));
    fs::create_dir_all(log_file.parent().unwrap())?;

    Ok(ResolvedLaunch {
        main_class,
        jvm_args: jvm_args_all,
        game_args,
        classpath,
        working_dir: game_dir,
        log_file,
    })
}

/// Reglas de inclusión de librerías de Mojang.
fn rules_apply(lib: &Value) -> bool {
    let Some(rules) = lib.get("rules").and_then(|v| v.as_array()) else {
        return true;
    };
    let mut allowed = true;
    for rule in rules {
        let action = rule.get("action").and_then(|v| v.as_str()).unwrap_or("allow");
        let os = rule.get("os").and_then(|v| v.as_object());
        let os_matches = os.map_or(true, |os| {
            let name_ok = os.get("name").and_then(|v| v.as_str()).map_or(true, |name| {
                matches_os(name)
            });
            let arch_ok = os.get("arch").and_then(|v| v.as_str()).map_or(true, |arch| {
                arch == target_arch()
            });
            name_ok && arch_ok
        });
        if os_matches {
            allowed = action == "allow";
        }
    }
    allowed
}

fn matches_os(name: &str) -> bool {
    let current = std::env::consts::OS;
    match name {
        "windows" => current == "windows",
        "osx" => current == "macos",
        "linux" => current == "linux",
        _ => false,
    }
}

fn native_classifier() -> &'static str {
    if cfg!(windows) {
        "natives-windows"
    } else if cfg!(target_os = "macos") {
        "natives-macos"
    } else {
        "natives-linux"
    }
}

fn target_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64"
    }
}

fn classpath_to_string(classpath: &[PathBuf]) -> String {
    let sep = if cfg!(windows) { ";" } else { ":" };
    classpath
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(sep)
}

/// Descarga un fichero si no existe localmente, verificando SHA-1 si se provee.
async fn download_file(url: &str, dest: &Path, sha1: Option<&str>) -> Result<()> {
    if dest.exists() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension("tmp");
    let mut response = reqwest::Client::new().get(url).send().await?.error_for_status()?;
    let mut file = fs::File::create(&tmp)?;
    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk)?;
    }
    drop(file);
    if let Some(expected) = sha1 {
        let actual = sha1_of_file(tmp.as_path())?;
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = fs::remove_file(&tmp);
            return Err(format!(
                "SHA-1 no coincide para {url}: esperado {expected}, obtenido {actual}"
            )
            .into());
        }
    }
    fs::rename(tmp, dest)?;
    Ok(())
}

fn sha1_of_file(file: &Path) -> Result<String> {
    let mut f = fs::File::open(file)?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Extrae un jar de nativos al directorio de destino.
fn extract_natives(jar: &Path, dest: &Path) -> Result<()> {
    let file = fs::File::open(jar)?;
    let mut zip = zip::ZipArchive::new(file)?;
    zip.extract(dest)?;
    Ok(())
}

/// Resuelve los argumentos de Mojang (array de strings u objetos con rules/value).
fn resolve_json_args(args: &[Value], ctx: &LaunchContext) -> Vec<String> {
    let mut out = Vec::new();
    for arg in args {
        match arg {
            Value::Array(items) => out.extend(resolve_json_args(items, ctx)),
            Value::Object(obj) => {
                if let Some(rules) = obj.get("rules") {
                    if !rules.is_array() || !rules_apply_value(rules) {
                        continue;
                    }
                }
                if let Some(value) = obj.get("value") {
                    match value {
                        Value::Array(items) => out.extend(resolve_json_args(items, ctx)),
                        Value::String(s) => out.push(substitute(s, ctx)),
                        _ => {}
                    }
                }
            }
            Value::String(s) => out.push(substitute(s, ctx)),
            _ => {}
        }
    }
    out
}

fn rules_apply_value(rules: &Value) -> bool {
    let empty: Vec<Value> = Vec::new();
    let arr = rules.as_array().unwrap_or(&empty);
    let mut allowed = true;
    for rule in arr {
        let action = rule.get("action").and_then(|v| v.as_str()).unwrap_or("allow");
        let os = rule.get("os").and_then(|v| v.as_object());
        let os_matches = os.map_or(true, |os| {
            let name_ok = os.get("name").and_then(|v| v.as_str()).map_or(true, |name| matches_os(name));
            let arch_ok = os.get("arch").and_then(|v| v.as_str()).map_or(true, |arch| arch == target_arch());
            name_ok && arch_ok
        });
        if os_matches {
            allowed = action == "allow";
        }
    }
    allowed
}

fn substitute(arg: &str, ctx: &LaunchContext) -> String {
    let mut s = arg.to_string();
    macro_rules! repl {
        ($pat:expr, $val:expr) => {{
            let value: String = ($val).to_string();
            s = s.replace($pat, &value);
        }};
    }
    repl!("${auth_player_name}", ctx.auth_player_name);
    repl!("${auth_uuid}", ctx.auth_uuid);
    repl!("${auth_access_token}", ctx.auth_access_token);
    repl!("${auth_session}", ctx.auth_access_token);
    repl!("${user_type}", ctx.user_type);
    repl!("${user_properties}", ctx.user_properties);
    repl!("${version_name}", ctx.version_name);
    repl!("${version_type}", ctx.version_type);
    repl!("${game_directory}", ctx.game_directory.to_string_lossy());
    repl!("${assets_root}", ctx.assets_root.to_string_lossy());
    repl!("${assets_index_name}", ctx.assets_index_name);
    repl!("${resolution_width}", ctx.resolution_width);
    repl!("${resolution_height}", ctx.resolution_height);
    repl!("${library_directory}", ctx.library_directory.to_string_lossy());
    repl!("${classpath}", ctx.classpath_value);
    repl!("${natives_directory}", ctx.natives_directory.to_string_lossy());
    s
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitute_replaces_placeholders() {
        let ctx = LaunchContext {
            version_name: "1.20.1".into(),
            version_type: "release".into(),
            game_directory: "/tmp/game".into(),
            assets_root: "/tmp/assets".into(),
            assets_index_name: "1.20".into(),
            auth_player_name: "Player".into(),
            auth_uuid: "00000000-0000-0000-0000-000000000000".into(),
            auth_access_token: "".into(),
            user_type: "mojang".into(),
            user_properties: "{}".into(),
            resolution_width: "854".into(),
            resolution_height: "480".into(),
            library_directory: "/tmp/lib".into(),
            classpath_value: "/tmp/cp".into(),
            natives_directory: "/tmp/natives".into(),
        };
        assert_eq!(
            substitute("${version_name} ${auth_player_name}", &ctx),
            "1.20.1 Player"
        );
        assert_eq!(
            substitute("--gameDir ${game_directory}", &ctx),
            "--gameDir /tmp/game"
        );
    }

    #[test]
    fn rules_apply_allows_matching_os() {
        let lib = serde_json::json!({
            "rules": [
                { "action": "allow", "os": { "name": "linux" } },
                { "action": "allow", "os": { "name": "windows" } }
            ]
        });
        // target_arch() puede ser x64 o aarch64; sin restricción de arch pasa en cualquier arch
        assert!(rules_apply(&lib));
    }
}
