#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod config;
mod error;
mod instance;
mod java;
mod launcher;
mod minecraft;
mod ram;
mod state;

use std::fs;
use std::sync::Mutex;

use tauri::Manager;

use crate::launcher::Launcher;
use crate::state::{AppState, Inner};

fn main() {
    tauri::Builder::default()
        .setup(|app| -> std::result::Result<(), Box<dyn std::error::Error>> {
            // Tauri 1 exige que el closure devuelva Result<(), Box<dyn Error>>.
            // Se usa `expect` en el interior para no tener que convertir cada
            // error concreto a ese tipo.
            let handle = app.handle();
            let data_dir = handle
                .path_resolver()
                .app_data_dir()
                .expect("No se pudo resolver el directorio de datos de la aplicación");

            fs::create_dir_all(data_dir.join("instances"))
                .expect("No se pudo crear data_dir/instances");
            fs::create_dir_all(data_dir.join("java"))
                .expect("No se pudo crear data_dir/java");
            fs::create_dir_all(data_dir.join("assets"))
                .expect("No se pudo crear data_dir/assets");
            fs::create_dir_all(data_dir.join("logs"))
                .expect("No se pudo crear data_dir/logs");

            let instances_dir = data_dir.join("instances");
            let java_dir = data_dir.join("java");
            let assets_dir = data_dir.join("assets");
            let config = crate::config::ConfigEngine::new(data_dir.clone())
                .expect("No se pudo inicializar el motor de configuración");

            let inner = Inner {
                data_dir,
                instances_dir: instances_dir.clone(),
                java_dir,
                assets_dir,
                config,
                launcher: Mutex::new(Launcher::new(instances_dir)),
            };
            app.handle().manage(AppState {
                inner: Mutex::new(inner),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_theme,
            commands::set_theme,
            commands::list_instances,
            commands::get_instance,
            commands::create_instance,
            commands::resolve_config,
            commands::set_global_config,
            commands::set_instance_config,
            commands::get_system_ram,
            commands::launch_instance,
            commands::kill_instance,
        ])
        .run(tauri::generate_context!())
        .expect("Error al ejecutar RF Launcher");
}
