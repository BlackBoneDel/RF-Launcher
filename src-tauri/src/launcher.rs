use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::error::{LauncherError, Result};
use crate::ram::process_exists;

/// Instancia de juego en ejecución (seguimiento en memoria únicamente).
pub struct RunningInstance {
    pub pid: u32,
    pub log_path: PathBuf,
}

/// Gestor de procesos de juego lanzados.
///
/// Diseño de ciclo de vida (documentado en el informe): el juego se lanza como
/// proceso independiente del launcher. El manejador de `std::process::Child`
/// se `forget` tras el spawn, por lo que:
///  * Cerrar el launcher NO mata el juego (el juego sigue vivo, adoptado por init).
///  * Al reiniciar el launcher se pierde el tracking de partidas en curso.
///  * "Matar instancia" funciona solo sobre lo que el launcher ha lanzado en
///    esta sesión.
///
/// Esta es la semántica esperada por usuarios de Minecraft; la alternativa
/// (matar el juego al cerrar el launcher) sería sorprendente y perdedora de
/// progreso.
pub struct Launcher {
    instances_dir: PathBuf,
    pub running: HashMap<Uuid, RunningInstance>,
}

impl Launcher {
    pub fn new(instances_dir: PathBuf) -> Self {
        Self {
            instances_dir,
            running: HashMap::new(),
        }
    }

    /// Lanza el proceso de Java con los argumentos ya resueltos.
    /// Devuelve el PID y registra la instancia como en ejecución.
    pub fn spawn(
        &mut self,
        instance: &Instance,
        java_path: &Path,
        ram_min_mb: u64,
        ram_max_mb: u64,
        working_dir: &Path,
        classpath: &[PathBuf],
        main_class: &str,
        jvm_extra_args: &[String],
        game_args: &[String],
        log_file: &Path,
    ) -> Result<u32> {
        if let Some(parent) = log_file.parent() {
            fs_create_dir_all(parent)?;
        }
        let log_path = log_file.to_path_buf();
        let log_file = File::create(&log_path)?;
        let log_err = log_file.try_clone()?;

        let mut cmd = Command::new(java_path);
        cmd.arg(format!("-Xms{min}m", min = ram_min_mb))
            .arg(format!("-Xmx{max}m", max = ram_max_mb));
        for arg in jvm_extra_args {
            cmd.arg(arg);
        }
        cmd.arg("-Dminecraft.launcher.launcher=rf-launcher")
            .arg("-cp")
            .arg(classpath_string(classpath))
            .arg(main_class);
        for arg in game_args {
            cmd.arg(arg);
        }
        cmd.current_dir(working_dir)
            .stdout(log_file)
            .stderr(log_err)
            .stdin(Stdio::null());

        let mut child = cmd.spawn()?;
        let pid = child
            .id()
            .ok_or_else(|| LauncherError::Custom("java no devolvió PID".into()))?;
        // Desvincular el proceso del launcher: el juego vive por su cuenta.
        std::mem::forget(child);

        self.running.insert(
            instance.id,
            RunningInstance {
                pid,
                log_path: log_file.to_path_buf(),
            },
        );
        Ok(pid)
    }

    /// Pide detener una instancia y verifica que el proceso realmente terminó.
    ///
    /// Flujo: SIGTERM (TerminateProcess en Windows), se espera hasta 10 s
    /// comprobando la existencia del PID vía sysinfo. Si persiste, se envía
    /// SIGKILL (taskkill /F en Windows) y se espera hasta 5 s más.
    /// Éxito solo se reporta si el PID desapareció de la lista de procesos.
    pub fn kill(&mut self, id: &Uuid) -> KillResult {
        let Some(slot) = self.running.remove(id) else {
            return KillResult {
                ok: true,
                terminated: false,
                error: None,
            };
        };

        let terminated = terminate_pid(slot.pid);

        KillResult {
            ok: terminated,
            terminated,
            error: if terminated {
                None
            } else {
                Some(format!("el proceso {pid} no terminó tras enviar la señal", pid = slot.pid))
            },
        }
    }

    pub fn is_running(&self, id: &Uuid) -> bool {
        self.running
            .get(id)
            .is_some_and(|slot| process_exists(slot.pid))
    }

    pub fn running_pids(&self) -> Vec<u32> {
        self.running.values().map(|s| s.pid).collect()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KillResult {
    pub ok: bool,
    pub terminated: bool,
    pub error: Option<String>,
}

/// Envía señal de terminación y confirma salida del proceso.
fn terminate_pid(pid: u32) -> bool {
    // Primer intento: señal cortés (SIGTERM / TerminateProcess).
    signal_pid(pid, false);
    if wait_until_gone(pid, Duration::from_secs(10)) {
        return true;
    }
    // Escalada: señal forzosa (SIGKILL / taskkill /F).
    signal_pid(pid, true);
    wait_until_gone(pid, Duration::from_secs(5))
}

/// Espera a que el PID desaparezca de la tabla de procesos del SO.
fn wait_until_gone(pid: u32, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if !process_exists(pid) {
            return true;
        }
        thread::sleep(Duration::from_millis(200));
    }
    !process_exists(pid)
}

/// Envía una señal al proceso por PID, de forma multiplataforma.
fn signal_pid(pid: u32, force: bool) {
    let _ = if cfg!(windows) {
        if force {
            Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F", "/T"])
                .status()
        } else {
            Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T"])
                .status()
        }
    } else {
        let signal = if force { "KILL" } else { "TERM" };
        Command::new("kill").args([format!("-{signal}"), &pid.to_string()]).status()
    };
    // Los errores se ignoran a propósito: la verificación real es
    // wait_until_gone (¿sigue existiendo el proceso?).
}

fn classpath_string(classpath: &[PathBuf]) -> String {
    let sep = if cfg!(windows) { ";" } else { ":" };
    classpath
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(sep)
}

fn fs_create_dir_all(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn kill_unknown_instance_returns_not_terminated() {
        let mut launcher = Launcher::new(temp_dir().join("rf-launcher-test-kill"));
        let id = Uuid::new_v4();
        let result = launcher.kill(&id);
        assert!(result.ok);
        assert!(!result.terminated);
        assert!(result.error.is_none());
    }
}
