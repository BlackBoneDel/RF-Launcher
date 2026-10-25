use thiserror::Error;
use tar::TarError;

/// Errores del núcleo del launcher. Todos se convierten en String para los
/// comandos de Tauri (la frontera con el frontend no tipifica errores).
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
    #[error("TAR: {0}")]
    Tar(#[from] TarError),
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
    #[error("RAM solicitada ({requested} MB) supera la RAM física ({total} MB)")]
    RamExceedsPhysical { requested: u64, total: u64 },
    #[error("{0}")]
    Custom(String),
}

impl From<&str> for LauncherError {
    fn from(s: &str) -> Self {
        LauncherError::Custom(s.to_string())
    }
}

pub type Result<T> = std::result::Result<T, LauncherError>;
