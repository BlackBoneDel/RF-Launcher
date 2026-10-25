use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::error::Result;
use crate::instance::Instance;
use crate::ram::process_exists;

/// Instancia de juego en ejecución (seguimiento en memoria únicamente).
pub struct RunningInstance {
    pub child: Child,
    pub pid: u32,
    pub log_path: PathBuf,
}

/// Gestor de procesos de juego lanzados.
///
/// DECISIÓN DE DISEÑO (documentada en el informe): el juego se lanza como
/// proceso independiente del launcher. Se conserva el `Child` en memoria
/// (NO se hace `mem::forget`), porque `drop(Child)` NO mata el proceso:
/// simplemente cierra el handle. Así, al cerrar el launcher el juego sigue
/// vivo (reparentado a init/systemd o con el handle liberado en Windows),
/// que es el comportamiento esperado por cualquier usuario de Minecraft.
///
/// Conservar el `Child` tiene además una ventaja crítica: permite `wait()`
/// para REAPARECER (reap) el proceso. Sin eso, en Unix un proceso muerto
/// queda como zombi y `sysinfo` seguiría listándolo, haciendo imposible
/// verificar de verdad que "matar instancia" funcionó.
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

    pub fn instances_dir(&self) -> &Path {
        &self.instances_dir
    }

    /// Lanza el proceso real de Java con los argumentos ya resueltos.
    /// Devuelve el PID y registra la instancia como en ejecución.
    #[allow(clippy::too_many_arguments)]
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
            std::fs::create_dir_all(parent)?;
        }
        let log_path = log_file.to_path_buf();
        let stdout = File::create(&log_path)?;
        let stderr = stdout.try_clone()?;

        let mut cmd = Command::new(java_path);
        cmd.arg(format!("-Xms{ram_min_mb}m"))
            .arg(format!("-Xmx{ram_max_mb}m"));
        for arg in jvm_extra_args {
            cmd.arg(arg);
        }
        cmd.arg("-Dminecraft.launcher.brand=rf-launcher")
            .arg("-cp")
            .arg(classpath_string(classpath))
            .arg(main_class);
        for arg in game_args {
            cmd.arg(arg);
        }
        cmd.current_dir(working_dir)
            .stdout(stdout)
            .stderr(stderr)
            .stdin(Stdio::null());

        let child = cmd.spawn()?;
        // En Rust actual, `Child::id()` devuelve u32 directamente (no Option).
        let pid = child.id();

        self.running.insert(
            instance.id,
            RunningInstance {
                child,
                pid,
                log_path,
            },
        );
        Ok(pid)
    }

    /// Pide detener una instancia y VERIFICA que el proceso terminó.
    ///
    /// Flujo multiplataforma:
    ///  1. Señal cortés: SIGTERM en Unix / `taskkill` sin /F en Windows.
    ///  2. Se espera hasta 5 s usando `Child::try_wait()`, que además REAPARECER
    ///     (reap) el hijo: sin esto quedaría un zombi en la tabla de procesos.
    ///  3. Si sigue vivo, escalada: `Child::kill()` (SIGKILL / TerminateProcess)
    ///     y `wait()`.
    ///  4. Verificación final: el PID debe haber DESAPARECIDO de la lista de
    ///     procesos del SO (sysinfo). Sólo entonces se reporta éxito.
    pub fn kill(&mut self, id: &Uuid) -> KillResult {
        let Some(mut slot) = self.running.remove(id) else {
            return KillResult {
                ok: true,
                terminated: false,
                error: None,
            };
        };

        let pid = slot.pid;
        let terminated = terminate_child(&mut slot.child, pid);

        KillResult {
            ok: terminated,
            terminated,
            error: if terminated {
                None
            } else {
                Some(format!(
                    "el proceso {pid} sigue en la tabla de procesos tras la señal de terminación"
                ))
            },
        }
    }

    pub fn is_running(&mut self, id: &Uuid) -> bool {
        match self.running.get_mut(id) {
            Some(slot) => matches!(slot.child.try_wait(), Ok(None)),
            None => false,
        }
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

/// Termina un hijo y confirma que su PID desapareció del sistema.
/// Público para poder probarlo con un proceso real en los 3 SO.
pub fn terminate_child(child: &mut Child, pid: u32) -> bool {
    // 1) Señal cortés (en Windows, taskkill sin /F suele no poder con procesos
    //    de consola; por eso la escalada es obligatoria y está cubierta).
    signal_pid(pid, false);

    // 2) Esperar la salida con try_wait(), que REAPARECER el proceso.
    if wait_for_exit(child, Duration::from_secs(5)) {
        return !process_exists(pid);
    }

    // 3) Escalada forzosa.
    let _ = child.kill();
    let _ = child.wait();

    // 4) Verificación explícita: el PID ya no debe estar en la tabla de procesos.
    wait_until_gone(pid, Duration::from_secs(5))
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        match child.try_wait() {
            Ok(Some(_status)) => return true,
            Ok(None) => thread::sleep(Duration::from_millis(200)),
            Err(_) => return false,
        }
    }
    false
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
/// Los errores se ignoran a propósito: la verificación real es
/// `wait_for_exit` + `wait_until_gone` (¿sigue existiendo el proceso?).
fn signal_pid(pid: u32, force: bool) {
    let pid_str = pid.to_string();
    if cfg!(windows) {
        let mut cmd = Command::new("taskkill");
        cmd.args(["/PID", &pid_str, "/T"]);
        if force {
            cmd.arg("/F");
        }
        let _ = cmd.stdout(Stdio::null()).stderr(Stdio::null()).status();
    } else {
        let signal = if force { "-KILL" } else { "-TERM" };
        let _ = Command::new("kill")
            .args([signal, &pid_str])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn classpath_string(classpath: &[PathBuf]) -> String {
    let sep = if cfg!(windows) { ";" } else { ":" };
    classpath
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(sep)
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

    /// Evidencia real del criterio de éxito #6: se lanza un proceso de
    /// larga duración, se le "mata" y se comprueba que su PID DESAPARECE
    /// de la tabla de procesos. Se ejecuta en los 3 SO vía la matriz de CI.
    #[test]
    fn kill_really_terminates_process_and_pid_disappears() {
        let mut child = if cfg!(windows) {
            Command::new("ping")
                .args(["-n", "60", "127.0.0.1"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("no se pudo lanzar ping")
        } else {
            Command::new("sleep")
                .arg("60")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("no se pudo lanzar sleep")
        };

        let pid = child.id(); // u32 directo, no Option
        assert!(
            process_exists(pid),
            "el proceso recién lanzado debe existir en la tabla de procesos"
        );

        assert!(
            terminate_child(&mut child, pid),
            "terminate_child debe confirmar la terminación"
        );
        assert!(
            !process_exists(pid),
            "el PID {pid} debe haber DESAPARECIDO de la lista de procesos"
        );
    }

    #[test]
    fn classpath_separator_is_platform_correct() {
        let paths = vec![PathBuf::from("/a/b.jar"), PathBuf::from("/c/d.jar")];
        let s = classpath_string(&paths);
        let expected_sep = if cfg!(windows) { ";" } else { ":" };
        assert!(s.contains(expected_sep));
    }
}
