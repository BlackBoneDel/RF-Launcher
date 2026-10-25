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

use crate::state::{AppState, Inner};

fn main() {
    tauri::Builder::default()
        .setup(|app| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let handle = app.handle();
            let data_dir = handle
                .path_resolver()
                .app_data_dir()
                .ok_or("No se pudo resolver el directorio de datos de la aplicación")?;

            fs::create_dir_all(data_dir.join("instances"))?;
            fs::create_dir_all(data_dir.join("java"))?;
            fs::create_dir_all(data_dir.join("assets"))?;
            fs::create_dir_all(data_dir.join("logs"))?;

            let instances_dir = data_dir.join("instances");
            let java_dir = data_dir.join("java");
            let assets_dir = data_dir.join("assets");
            let config = crate::config::ConfigEngine::new(data_dir.clone())?;
            let inner = Inner {
                data_dir,
                instances_dir,
                java_dir,
                assets_dir,
                config,
                launcher: Mutex::new(Launcher::new(instances_dir.clone())),
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
        .run(tauri::generate_context!());
}
