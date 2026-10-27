use serde::ser::Serializer;
use thiserror::Error;

/// Errores del núcleo del launcher.
///
/// Tauri 2 exige que TODO lo que devuelve un comando implemente
/// `serde::Serialize`, incluido el tipo de error. En lugar de convertir cada
/// error a String en cada comando, serializamos el error como una cadena
/// legible (su mensaje `Display`).
#[derive(Error, Debug)]
pub enum LauncherError {
    #[error("E/S: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("HTTP: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("ZIP: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Instancia no encontrada: {0}")]
    InstanceNotFound(String),
    #[error("Configuración desconocida: {0}")]
    UnknownConfigKey(String),
    #[error("UUID inválido: {0}")]
    InvalidUuid(String),
    #[error("La versión de Minecraft \"{0}\" no tiene mapeo de Java conocido")]
    UnknownJavaMapping(String),
    #[error("La descarga de Java {0} falló")]
    JavaDownloadFailed(u32),
    #[error("RAM solicitada ({requested} MB) alcanza o supera la RAM física ({total} MB); 100 % o más se rechaza en duro")]
    RamExceedsPhysical { requested: u64, total: u64 },
    #[error("{0}")]
    Custom(String),
}

impl From<&str> for LauncherError {
    fn from(s: &str) -> Self {
        LauncherError::Custom(s.to_string())
    }
}

impl From<String> for LauncherError {
    fn from(s: String) -> Self {
        LauncherError::Custom(s)
    }
}

impl serde::Serialize for LauncherError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, LauncherError>;
