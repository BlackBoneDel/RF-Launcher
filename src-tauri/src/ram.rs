use sysinfo::{Pid, System};

/// RAM física total de la máquina en MB. sysinfo lee el valor real del SO
/// (Linux: /proc/meminfo; Windows: GlobalMemoryStatusEx; macOS: hw.memsize).
pub fn total_ram_mb() -> u64 {
    let mut system = System::new_all();
    system.total_memory() / 1024 / 1024
}

/// Resultado de validar una cantidad de RAM solicitada contra la física.
pub enum RamValidation {
    /// La solicitud es segura.
    Ok,
    /// Advertencia blanda: la solicitud supera el umbral imprudente (80% de la
    /// RAM física) pero no la supera. El usuario debe confirmar explícitamente.
    Warn { requested_mb: u64, total_mb: u64 },
    /// Error duro: se pide más RAM de la que tiene la máquina.
    Exceeds { requested_mb: u64, total_mb: u64 },
}

/// Umbral por encima del cual se considera la asignación imprudente.
const RAM_WARN_THRESHOLD: f64 = 0.80;

pub fn validate_ram_max(requested_mb: u64, total_mb: u64) -> RamValidation {
    if requested_mb > total_mb {
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
    let mut system = System::new_all();
    system.process(Pid::from_u32(pid)).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(matches!(
            validate_ram_max(32000, 32000),
            RamValidation::Exceeds { .. }
        ));
    }
}
