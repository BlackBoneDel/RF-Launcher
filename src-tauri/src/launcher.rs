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
    pub running: HashMap<Uuid, RunningInstance>,
}

impl Launcher {
    pub fn new() -> Self {
        Self {
            running: HashMap::new(),
        }
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
            RunningInstance { child, pid },
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

}

/// Resultado de detener una instancia.
///
/// SÓLO IPC: se devuelve al frontend y nunca se persiste en disco, caché o
/// log. Sus claves son palabras simples (`ok`, `terminated`, `error`), por lo
/// que no hay ficheros legacy snake_case y no necesita `alias`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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
        let mut launcher = Launcher::new();
        let id = Uuid::new_v4();
        let result = launcher.kill(&id);
        assert!(result.ok);
        assert!(!result.terminated);
        assert!(result.error.is_none());
    }

    #[test]
    fn kill_result_wire_fields_are_camel_case() {
        let result = KillResult {
            ok: true,
            terminated: true,
            error: None,
        };
        let obj = serde_json::to_value(&result).unwrap();
        let keys: Vec<&str> = obj.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(obj["ok"], true);
        assert_eq!(obj["terminated"], true);
        assert_eq!(obj["error"], serde_json::Value::Null);
        assert!(
            keys.iter().all(|k| !k.contains('_')),
            "ningún campo snake_case en KillResult: {keys:?}"
        );

        let back: KillResult = serde_json::from_value(obj).unwrap();
        assert!(back.ok);
        assert!(back.terminated);
        assert_eq!(back.error, None);
    }

    /// Ramificación HIJO del test de kill (Parte H de la auditoría): en vez
    /// de un `sleep`/`ping` trivial, el proceso víctima es el propio binario
    /// de tests con 96 MiB de memoria REALMENTE tocada y 4 hilos activos.
    /// Escribe un fichero de "listo" DESPUÉS de reservar todo, de modo que el
    /// padre nunca mate a un proceso vacío.
    fn run_realistic_child() {
        let mut blocks: Vec<Vec<u8>> = Vec::new();
        for _ in 0..96 {
            let mut block = vec![0u8; 1024 * 1024];
            // Tocar TODAS las páginas: `vec![0u8; N]` usa memoria zeroed que
            // el SO puede mapear de forma perezosa. Escribir solo 2 bytes por
            // bloque dejaba el RSS real en ~10 MB (medido en CI), no en 96 MB.
            let mut i = 0usize;
            while i < block.len() {
                block[i] = 0x5A;
                i += 4096;
            }
            let last = block.len() - 1;
            block[last] = 0xA5;
            blocks.push(block);
        }
        assert_eq!(blocks.len(), 96);

        let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let workers: Vec<_> = (0..4)
            .map(|_| {
                let flag = alive.clone();
                thread::spawn(move || {
                    while flag.load(std::sync::atomic::Ordering::Relaxed) {
                        thread::sleep(Duration::from_millis(100));
                    }
                })
            })
            .collect();

        if let Some(path) = std::env::var_os("RF_LAUNCHER_CHILD_READY") {
            let _ = std::fs::write(
                path,
                format!("96MB_4THREADS threads={}\n", workers.len()),
            );
        }

        // El padre debe matarnos mucho antes; 120 s es sólo un límite de
        // seguridad para no dejar un proceso colgado si el test falla.
        thread::sleep(Duration::from_secs(120));
        alive.store(false, std::sync::atomic::Ordering::Relaxed);
        for worker in workers {
            let _ = worker.join();
        }
    }

    /// Evidencia real del criterio de éxito #6: se lanza un proceso con
    /// memoria e hilos, se le "mata" y se comprueba que su PID DESAPARECE
    /// de la tabla de procesos. Se ejecuta en los 3 SO vía la matriz de CI.
    #[test]
    fn kill_really_terminates_process_and_pid_disappears() {
        if std::env::var_os("RF_LAUNCHER_CHILD_MODE").is_some() {
            run_realistic_child();
            return;
        }

        let ready = temp_dir().join(format!("rf-kill-ready-{}", std::process::id()));
        let _ = std::fs::remove_file(&ready);

        let exe = std::env::current_exe().expect("no se pudo resolver el binario de tests");
        let mut child = Command::new(exe)
            .arg("kill_really_terminates_process_and_pid_disappears")
            .arg("--nocapture")
            .env("RF_LAUNCHER_CHILD_MODE", "1")
            .env("RF_LAUNCHER_CHILD_READY", &ready)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("no se pudo relanzar el binario de tests como proceso víctima");

        let pid = child.id(); // u32 directo, no Option
        let start = Instant::now();
        while !ready.exists() && start.elapsed() < Duration::from_secs(20) {
            thread::sleep(Duration::from_millis(100));
        }
        assert!(
            ready.exists(),
            "el hijo no llegó a reservar 96 MiB y 4 hilos en 20 s"
        );

        // Evidencia medible de que el proceso NO es trivial.
        let sys = sysinfo::System::new_all();
        let rss_kb = sys
            .process(sysinfo::Pid::from_u32(pid))
            .map(|p| p.memory() / 1024)
            .unwrap_or(0);
        println!("KILL_TEST_CHILD_RSS_KB={rss_kb}");

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
        let _ = std::fs::remove_file(&ready);
    }

    #[test]
    fn classpath_separator_is_platform_correct() {
        let paths = vec![PathBuf::from("/a/b.jar"), PathBuf::from("/c/d.jar")];
        let s = classpath_string(&paths);
        let expected_sep = if cfg!(windows) { ";" } else { ":" };
        assert!(s.contains(expected_sep));
    }
}
