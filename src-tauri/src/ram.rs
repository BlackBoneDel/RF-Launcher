use sysinfo::{Pid, System};

/// RAM física total de la máquina en MB. sysinfo lee el valor real del SO
/// (Linux: /proc/meminfo; Windows: GlobalMemoryStatusEx; macOS: hw.memsize).
pub fn total_ram_mb() -> u64 {
    let system = System::new_all();
    system.total_memory() / 1024 / 1024
}

/// Resultado de validar una cantidad de RAM solicitada contra la física.
pub enum RamValidation {
    /// La solicitud es segura.
    Ok,
    /// Advertencia blanda: supera el umbral imprudente (80% de la RAM física)
    /// pero sigue siendo estrictamente menor que el 100%. Se puede confirmar
    /// explícitamente con `force`.
    Warn { requested_mb: u64, total_mb: u64 },
    /// Error duro: se pide el 100% o más de la RAM física.
    Exceeds { requested_mb: u64, total_mb: u64 },
}

/// Umbral por encima del cual se considera la asignación imprudente.
const RAM_WARN_THRESHOLD: f64 = 0.80;

/// DECISIÓN FINAL (issue #5, ratificada por el usuario): pedir exactamente el
/// 100% de la RAM física TAMBIÉN se rechaza en duro. La comparación es `>=`,
/// no `>`: el SO necesita memoria y un heap JVM de toda la RAM física es un
/// fallo en tiempo de ejecución, no una imprudencia confirmable. `force` sólo
/// puede confirmar la franja de aviso (>80% y <100%).
///
/// Todas las rutas que validan RAM (config global, override por instancia y
/// lanzamiento) deben usar esta función para no divergir en el límite.
pub fn validate_ram_max(requested_mb: u64, total_mb: u64) -> RamValidation {
    if requested_mb >= total_mb {
        RamValidation::Exceeds {
            requested_mb,
            total_mb,
        }
    } else if (requested_mb as f64) > (total_mb as f64) * RAM_WARN_THRESHOLD {
        RamValidation::Warn {
            requested_mb,
            total_mb,
        }
    } else {
        RamValidation::Ok
    }
}

/// Comprueba si un PID todavía existe en el sistema (usado para verificar que
/// "matar instancia" realmente terminó el proceso).
pub fn process_exists(pid: u32) -> bool {
    // `System::new_all()` carga también la tabla de procesos (verificado en
    // docs.rs/sysinfo/0.32.1). Se usa tanto en Linux como en Windows/macOS.
    let system = System::new_all();
    system.process(Pid::from_u32(pid)).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Este test no sólo comprueba que la detección funciona: IMPRIME el valor
    /// real leído por sysinfo en cada SO de la matriz de CI, de modo que la
    /// línea base de recursos queda registrada como evidencia verificable.
    #[test]
    fn ram_detection_reads_real_physical_memory() {
        let mb = total_ram_mb();
        println!("RAM_FISICA_MB={mb}");
        assert!(mb > 0, "sysinfo debe leer RAM física positiva en este SO");
        assert!(mb < 2_000_000, "valor improbable: {mb} MB");
    }

    #[test]
    fn ram_exceeds_physical_is_rejected() {
        assert!(matches!(
            validate_ram_max(90000, 32000),
            RamValidation::Exceeds { .. }
        ));
    }

    #[test]
    fn ram_over_80_percent_warns() {
        assert!(matches!(
            validate_ram_max(26000, 32000),
            RamValidation::Warn { .. }
        ));
    }

    #[test]
    fn ram_under_80_percent_is_ok() {
        assert!(matches!(
            validate_ram_max(2048, 32000),
            RamValidation::Ok
        ));
    }

    #[test]
    fn ram_equal_to_physical_is_rejected() {
        // Criterio final ratificado: 100% exacto es `Exceeds`, no `Warn`.
        assert!(matches!(
            validate_ram_max(32000, 32000),
            RamValidation::Exceeds { .. }
        ));
        // Un MB por encima también es rechazo duro.
        assert!(matches!(
            validate_ram_max(32001, 32000),
            RamValidation::Exceeds { .. }
        ));
    }
}
