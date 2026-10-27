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
/// siempre se persiste como null. En el cable JSON el campo se llama
/// `templateOrigin` (camelCase), no `template_origin`: ver el atributo serde
/// y el test de regresión `template_origin_wire_field_is_exactly_...`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: Uuid,
    pub name: String,
    pub minecraft_version: String,
    pub fabric_loader: Option<String>,
    /// Nombre de la plantilla de origen. En el cable Rust -> TypeScript se
    /// llama EXACTAMENTE `templateOrigin` (el plan original de Fase 3-A lo
    /// especificaba en camelCase). `alias` permite leer sin migración los
    /// `instance.json` escritos antes de esta corrección.
    #[serde(rename = "templateOrigin", alias = "template_origin")]
    pub template_origin: Option<String>,
    /// Overrides de configuración específicos de esta instancia (ej. RAM).
    /// Las claves viven en config::ConfigKey, serializadas como strings.
    pub config_overrides: HashMap<String, ConfigValue>,
    pub ram_min_mb: Option<u64>,
    pub ram_max_mb: Option<u64>,
    pub created_at: u64,
    pub last_launched_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceSummary {
    pub id: String,
    pub name: String,
    pub minecraft_version: String,
    pub fabric_loader: Option<String>,
    #[serde(rename = "templateOrigin", alias = "template_origin")]
    pub template_origin: Option<String>,
    pub ram_min_mb: Option<u64>,
    pub ram_max_mb: Option<u64>,
    pub created_at: u64,
    pub last_launched_at: Option<u64>,
}

impl Instance {
    /// Carpeta de la instancia en disco: data_dir/instances/<uuid>/
    pub fn dir(instances_dir: &Path, id: &Uuid) -> PathBuf {
        instances_dir.join(id.to_string())
    }

    /// Carpeta de datos del juego de esta instancia (`.minecraft` aislado).
    pub fn game_dir(&self, instances_dir: &Path) -> PathBuf {
        Self::dir(instances_dir, &self.id).join("minecraft")
    }

    /// Carpeta de logs de lanzamientos de esta instancia.
    pub fn logs_dir(&self, instances_dir: &Path) -> PathBuf {
        Self::dir(instances_dir, &self.id).join("logs")
    }

    /// Ruta del fichero de persistencia de la instancia.
    pub fn config_path(&self, instances_dir: &Path) -> PathBuf {
        Self::dir(instances_dir, &self.id).join("instance.json")
    }

    pub fn summary(&self) -> InstanceSummary {
        InstanceSummary {
            id: self.id.to_string(),
            name: self.name.clone(),
            minecraft_version: self.minecraft_version.clone(),
            fabric_loader: self.fabric_loader.clone(),
            template_origin: self.template_origin.clone(),
            ram_min_mb: self.ram_min_mb,
            ram_max_mb: self.ram_max_mb,
            created_at: self.created_at,
            last_launched_at: self.last_launched_at,
        }
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
        assert_eq!(parsed["minecraft_version"], "1.20.1");
        assert_eq!(parsed["fabric_loader"], serde_json::Value::Null);

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

        // `InstanceSummary` debe usar la misma convención.
        let summary = serde_json::to_value(inst.summary()).unwrap();
        assert_eq!(summary["templateOrigin"], "plantilla-ejemplo");
        assert!(!summary.as_object().unwrap().contains_key("template_origin"));

        // El alias permite leer ficheros escritos con el nombre antiguo.
        let mut legacy = obj.clone();
        legacy.remove("templateOrigin");
        legacy.insert("template_origin".into(), serde_json::json!("legacy"));
        let back: Instance = serde_json::from_value(serde_json::Value::Object(legacy)).unwrap();
        assert_eq!(back.template_origin.as_deref(), Some("legacy"));
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
