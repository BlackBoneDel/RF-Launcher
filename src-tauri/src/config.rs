use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::error::{LauncherError, Result};

/// Claves de configuración tipadas. Se evitan strings mágicos sueltos: cualquier
/// clave de configuración del launcher vive aquí. Añadir una clave nueva (por
/// ejemplo, una opción específica de mods en Fase 3-B) solo requiere extender
/// este enum y su valor por defecto; la lógica de resolución no se toca.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ConfigKey {
    RamMinMb,
    RamMaxMb,
    JavaAutoDownload,
    Theme,
}

impl ConfigKey {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfigKey::RamMinMb => "RamMinMb",
            ConfigKey::RamMaxMb => "RamMaxMb",
            ConfigKey::JavaAutoDownload => "JavaAutoDownload",
            ConfigKey::Theme => "Theme",
        }
    }

    pub fn from_str(s: &str) -> Option<ConfigKey> {
        match s {
            "RamMinMb" => Some(ConfigKey::RamMinMb),
            "RamMaxMb" => Some(ConfigKey::RamMaxMb),
            "JavaAutoDownload" => Some(ConfigKey::JavaAutoDownload),
            "Theme" => Some(ConfigKey::Theme),
            _ => None,
        }
    }

    /// Valor por defecto del sistema para esta clave. Centralizado: nunca
    /// hardcodeado en dos sitios distintos.
    pub fn default_value(&self) -> ConfigValue {
        match self {
            ConfigKey::RamMinMb => ConfigValue::Int(1024),
            ConfigKey::RamMaxMb => ConfigValue::Int(2048),
            ConfigKey::JavaAutoDownload => ConfigValue::Bool(true),
            ConfigKey::Theme => ConfigValue::String("light".to_string()),
        }
    }
}

/// Valor de configuración, serializado con etiqueta para que la frontera
/// con el frontend (JSON) sea autocontenida.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "value")]
pub enum ConfigValue {
    Int(i64),
    Bool(bool),
    String(String),
}

impl ConfigValue {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            ConfigValue::Int(v) => Some(*v),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ConfigValue::Bool(v) => Some(*v),
            _ => None,
        }
    }
    pub fn as_string(&self) -> Option<&str> {
        match self {
            ConfigValue::String(v) => Some(v),
            _ => None,
        }
    }
}

/// Configuración global del launcher (persistida en data_dir/config.json).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalConfig {
    pub overrides: HashMap<String, ConfigValue>,
}

/// Motor de resolución jerárquica: instancia > global > defecto del sistema.
///
/// La resolución es una función pura sobre los override de la instancia y la
/// configuración global, lo que la hace trivial de probar unitariamente y de
/// extender sin tocar llamadores.
pub struct ConfigEngine {
    data_dir: PathBuf,
    global: Mutex<GlobalConfig>,
}

impl ConfigEngine {
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        let path = data_dir.join("config.json");
        let global = if path.exists() {
            let text = fs::read_to_string(&path)?;
            serde_json::from_str(&text)?
        } else {
            GlobalConfig::default()
        };
        Ok(Self {
            data_dir,
            global: Mutex::new(global),
        })
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    /// Guarda la configuración global en disco.
    pub fn save_global(&self) -> Result<()> {
        let global = self.global.lock().map_err(|e| LauncherError::Custom(e.to_string()))?;
        let json = serde_json::to_string_pretty(&*global)?;
        fs::write(self.data_dir.join("config.json"), json)?;
        Ok(())
    }

    pub fn global_overrides(&self) -> Result<HashMap<String, ConfigValue>> {
        Ok(self
            .global
            .lock()
            .map_err(|e| LauncherError::Custom(e.to_string()))?
            .overrides
            .clone())
    }

    pub fn set_global(&self, key: ConfigKey, value: ConfigValue) -> Result<()> {
        self.global
            .lock()
            .map_err(|e| LauncherError::Custom(e.to_string()))?
            .overrides
            .insert(key.as_str().to_string(), value);
        self.save_global()
    }

    /// Resuelve el valor efectivo para una instancia dada.
    /// Orden: override de la instancia > override global > defecto del sistema.
    pub fn resolve(
        &self,
        instance_overrides: &HashMap<String, ConfigValue>,
        key: ConfigKey,
    ) -> ConfigValue {
        instance_overrides
            .get(key.as_str())
            .cloned()
            .or_else(|| {
                self.global
                    .lock()
                    .map_err(|e| LauncherError::Custom(e.to_string()))
                    .ok()
                    .and_then(|g| g.overrides.get(key.as_str()).cloned())
            })
            .unwrap_or_else(|| key.default_value())
    }
}

/// Resuelve el tema visual (global, sin instancia). Wrapper de conveniencia.
pub fn resolve_theme(engine: &ConfigEngine) -> Result<String> {
    let overrides = HashMap::new();
    Ok(engine
        .resolve(&overrides, ConfigKey::Theme)
        .as_string()
        .unwrap_or("light")
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_overrides() -> HashMap<String, ConfigValue> {
        HashMap::new()
    }

    #[test]
    fn resolve_instance_override_wins_over_global() {
        let mut overrides = empty_overrides();
        overrides.insert("RamMaxMb".to_string(), ConfigValue::Int(8192));

        let mut global = GlobalConfig::default();
        global.overrides.insert("RamMaxMb".to_string(), ConfigValue::Int(4096));

        let engine = ConfigEngine {
            data_dir: PathBuf::from("/tmp"),
            global: Mutex::new(global),
        };

        let value = engine.resolve(&overrides, ConfigKey::RamMaxMb);
        assert_eq!(value, ConfigValue::Int(8192));
    }

    #[test]
    fn resolve_falls_back_to_global_when_instance_does_not_override() {
        let overrides = empty_overrides();

        let mut global = GlobalConfig::default();
        global.overrides.insert("RamMaxMb".to_string(), ConfigValue::Int(4096));

        let engine = ConfigEngine {
            data_dir: PathBuf::from("/tmp"),
            global: Mutex::new(global),
        };

        let value = engine.resolve(&overrides, ConfigKey::RamMaxMb);
        assert_eq!(value, ConfigValue::Int(4096));
    }

    #[test]
    fn resolve_falls_back_to_system_default_when_neither_defines_key() {
        let overrides = empty_overrides();
        let global = GlobalConfig::default();

        let engine = ConfigEngine {
            data_dir: PathBuf::from("/tmp"),
            global: Mutex::new(global),
        };

        // Ninguno define JavaAutoDownload -> defecto del sistema (true)
        let value = engine.resolve(&overrides, ConfigKey::JavaAutoDownload);
        assert_eq!(value, ConfigValue::Bool(true));

        // Tampoco definido en global -> defecto del sistema para Theme ("light")
        let value = engine.resolve(&overrides, ConfigKey::Theme);
        assert_eq!(value, ConfigValue::String("light".to_string()));

        // RamMaxMb por defecto del sistema
        let value = engine.resolve(&overrides, ConfigKey::RamMaxMb);
        assert_eq!(value, ConfigValue::Int(2048));
    }

    #[test]
    fn default_values_are_never_missing() {
        for key in [
            ConfigKey::RamMinMb,
            ConfigKey::RamMaxMb,
            ConfigKey::JavaAutoDownload,
            ConfigKey::Theme,
        ] {
            let value = key.default_value();
            assert!(
                matches!(value, ConfigValue::Int(_) | ConfigValue::Bool(_) | ConfigValue::String(_)),
                "clave {:?} debe tener defecto, obtuvo {:?}",
                key,
                value
            );
        }
    }
}
