use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::config::{ConfigKey, ConfigValue};
use crate::error::{LauncherError, Result};

/// Una instancia de Minecraft aislada: versión, loader, overrides de
/// configuración y su propia carpeta de datos en disco.
///
/// El identificador (UUID) es inmutable y es la clave de referencia; el nombre
/// visible puede cambiar sin romper nada. `template_origin` existe para fases
/// futuras (plantillas preconfiguradas, ej. "rf-client-default"); en esta fase
/// siempre se persiste como null y, por ahora, no se muestra en la UI.
///
/// CONVENCIÓN IPC (issue #6): todos los campos serializados usan camelCase.
/// Cada campo que antes salía en snake_case conserva un `alias` para poder
/// leer `instance.json` escritos por versiones anteriores sin migración.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: Uuid,
    pub name: String,
    #[serde(alias = "minecraft_version")]
    pub minecraft_version: String,
    #[serde(alias = "fabric_loader")]
    pub fabric_loader: Option<String>,
    /// Nombre de la plantilla de origen: en el cable es `templateOrigin`.
    #[serde(alias = "template_origin")]
    pub template_origin: Option<String>,
    /// Overrides de configuración específicos de esta instancia (ej. RAM).
    /// Las claves viven en config::ConfigKey, serializadas como strings.
    #[serde(alias = "config_overrides")]
    pub config_overrides: HashMap<String, ConfigValue>,
    #[serde(alias = "ram_min_mb")]
    pub ram_min_mb: Option<u64>,
    #[serde(alias = "ram_max_mb")]
    pub ram_max_mb: Option<u64>,
    #[serde(alias = "created_at")]
    pub created_at: u64,
    #[serde(alias = "last_launched_at")]
    pub last_launched_at: Option<u64>,
}

impl Instance {
    /// Carpeta de la instancia en disco: data_dir/instances/<uuid>/
    pub fn dir(instances_dir: &Path, id: &Uuid) -> PathBuf {
        instances_dir.join(id.to_string())
    }

    /// Ruta del fichero de persistencia de la instancia.
    pub fn config_path(&self, instances_dir: &Path) -> PathBuf {
        Self::dir(instances_dir, &self.id).join("instance.json")
    }

    /// Crea una instancia nueva y la persiste en disco.
    pub fn create(
        instances_dir: &Path,
        name: String,
        minecraft_version: String,
        fabric_loader: Option<String>,
    ) -> Result<Self> {
        if name.trim().is_empty() {
            return Err("El nombre de la instancia no puede estar vacío".into());
        }
        let id = Uuid::new_v4();
        let dir = Self::dir(instances_dir, &id);
        fs::create_dir_all(dir.join("minecraft"))?;
        fs::create_dir_all(dir.join("logs"))?;

        let now = now_secs();
        let instance = Self {
            id,
            name,
            minecraft_version,
            fabric_loader,
            template_origin: None,
            config_overrides: HashMap::new(),
            ram_min_mb: None,
            ram_max_mb: None,
            created_at: now,
            last_launched_at: None,
        };
        instance.save(instances_dir)?;
        Ok(instance)
    }

    /// Persiste la instancia en su fichero instance.json.
    pub fn save(&self, instances_dir: &Path) -> Result<()> {
        let path = self.config_path(instances_dir);
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Carga una instancia desde disco.
    pub fn load(instances_dir: &Path, id: &Uuid) -> Result<Self> {
        let path = Self::dir(instances_dir, id).join("instance.json");
        let text = fs::read_to_string(&path)
            .map_err(|_| LauncherError::InstanceNotFound(id.to_string()))?;
        serde_json::from_str(&text).map_err(Into::into)
    }

    /// Lista todas las instancias persistidas en disco.
    pub fn list(instances_dir: &Path) -> Result<Vec<Self>> {
        let mut out = Vec::new();
        if !instances_dir.exists() {
            return Ok(out);
        }
        for entry in fs::read_dir(instances_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let Ok(id) = parse_id(&path) else { continue };
                if let Ok(instance) = Self::load(instances_dir, &id) {
                    out.push(instance);
                }
            }
        }
        out.sort_by_key(|i| i.created_at);
        Ok(out)
    }

    pub fn set_override(&mut self, key: ConfigKey, value: ConfigValue) {
        self.config_overrides.insert(key.as_str().to_string(), value);
    }

    pub fn mark_launched(&mut self) {
        self.last_launched_at = Some(now_secs());
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn parse_id(path: &Path) -> Result<Uuid> {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| LauncherError::InvalidUuid(path.display().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn instance_serializes_template_origin_as_null() {
        let dir = temp_dir().join("rf-launcher-test-instance");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let inst = Instance::create(&dir, "test".into(), "1.20.1".into(), None).unwrap();
        let text = fs::read_to_string(inst.config_path(&dir)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();

        assert_eq!(parsed["templateOrigin"], serde_json::Value::Null);
        assert_eq!(parsed["id"], inst.id.to_string());
        assert_eq!(parsed["minecraftVersion"], "1.20.1");
        assert_eq!(parsed["fabricLoader"], serde_json::Value::Null);
        assert_eq!(parsed["configOverrides"], serde_json::json!({}));
        assert!(parsed.as_object().unwrap().keys().all(|k| !k.contains('_')));

        // Recarga desde disco y comprueba que template_origin sigue siendo None
        let reloaded = Instance::load(&dir, &inst.id).unwrap();
        assert_eq!(reloaded.template_origin, None);

        let _ = fs::remove_dir_all(&dir);
    }

    /// Regresión de protocolo: fija el nombre EXACTO del campo que cruza
    /// Rust -> TypeScript. Si alguien renombra la struct o quita el atributo
    /// serde, este test falla en los 3 SO en lugar de romper en silencio.
    #[test]
    fn template_origin_wire_field_is_exactly_template_origin_camel_case() {
        let inst = Instance {
            id: Uuid::new_v4(),
            name: "wire".into(),
            minecraft_version: "1.20.1".into(),
            fabric_loader: None,
            template_origin: Some("plantilla-ejemplo".into()),
            config_overrides: HashMap::new(),
            ram_min_mb: None,
            ram_max_mb: None,
            created_at: 1,
            last_launched_at: None,
        };

        let value = serde_json::to_value(&inst).unwrap();
        let obj = value.as_object().unwrap();
        assert_eq!(obj["templateOrigin"], "plantilla-ejemplo");
        assert!(
            !obj.contains_key("template_origin"),
            "el cable NO debe usar snake_case: {obj:?}"
        );

        // El alias permite leer ficheros escritos con el nombre antiguo.
        let mut legacy = obj.clone();
        legacy.remove("templateOrigin");
        legacy.insert("template_origin".into(), serde_json::json!("legacy"));
        let back: Instance = serde_json::from_value(serde_json::Value::Object(legacy)).unwrap();
        assert_eq!(back.template_origin.as_deref(), Some("legacy"));
    }

    #[test]
    fn instance_wire_fields_are_camel_case_and_legacy_aliases_work() {
        let id = Uuid::new_v4();
        let mut overrides = HashMap::new();
        overrides.insert("RamMaxMb".to_string(), ConfigValue::Int(4096));
        let inst = Instance {
            id,
            name: "wire".into(),
            minecraft_version: "1.20.1".into(),
            fabric_loader: Some("0.14.9".into()),
            template_origin: Some("plantilla".into()),
            config_overrides: overrides,
            ram_min_mb: Some(1024),
            ram_max_mb: Some(4096),
            created_at: 10,
            last_launched_at: Some(20),
        };

        let obj = serde_json::to_value(&inst).unwrap();
        let keys: Vec<&str> = obj.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(obj["minecraftVersion"], "1.20.1");
        assert_eq!(obj["fabricLoader"], "0.14.9");
        assert_eq!(obj["templateOrigin"], "plantilla");
        assert_eq!(obj["configOverrides"]["RamMaxMb"]["kind"], "Int");
        assert_eq!(obj["ramMinMb"], 1024);
        assert_eq!(obj["ramMaxMb"], 4096);
        assert_eq!(obj["createdAt"], 10);
        assert_eq!(obj["lastLaunchedAt"], 20);
        assert!(
            keys.iter().all(|k| !k.contains('_')),
            "ninguna clave del cable puede ser snake_case: {keys:?}"
        );

        let legacy = serde_json::json!({
            "id": id.to_string(),
            "name": "legacy",
            "minecraft_version": "1.16.5",
            "fabric_loader": "0.13.3",
            "template_origin": "legacy-template",
            "config_overrides": {"RamMaxMb": {"kind": "Int", "value": 8192}},
            "ram_min_mb": 512,
            "ram_max_mb": 8192,
            "created_at": 1,
            "last_launched_at": 2
        });
        let back: Instance = serde_json::from_value(legacy).unwrap();
        assert_eq!(back.minecraft_version, "1.16.5");
        assert_eq!(back.fabric_loader.as_deref(), Some("0.13.3"));
        assert_eq!(back.template_origin.as_deref(), Some("legacy-template"));
        assert_eq!(
            back.config_overrides.get("RamMaxMb"),
            Some(&ConfigValue::Int(8192))
        );
        assert_eq!(back.ram_min_mb, Some(512));
        assert_eq!(back.ram_max_mb, Some(8192));
        assert_eq!(back.created_at, 1);
        assert_eq!(back.last_launched_at, Some(2));
    }

    #[test]
    fn list_returns_only_valid_instance_dirs() {
        let dir = temp_dir().join("rf-launcher-test-list");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let _ = Instance::create(&dir, "alpha".into(), "1.20.1".into(), None).unwrap();
        let _ = Instance::create(&dir, "beta".into(), "1.16.5".into(), Some("0.14.9".into())).unwrap();
        // Carpeta no válida (no tiene instance.json): debe ignorarse
        fs::create_dir_all(dir.join("not-an-instance")).unwrap();

        let list = Instance::list(&dir).unwrap();
        assert_eq!(list.len(), 2);
        let ids: Vec<String> = list.iter().map(|i| i.id.to_string()).collect();
        assert!(ids.contains(&list[0].id.to_string()));

        let _ = fs::remove_dir_all(&dir);
    }
}
