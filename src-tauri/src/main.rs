#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod chat;
mod commands;
mod lm;
mod pipeline;
mod repo;
mod state;
mod xdp;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // Системная папка data/: SLED база, клоны репозиториев, сгенерированные .md
            let base = app.path().app_data_dir().expect("app_data_dir недоступна");
            let data = base.join("data");
            std::fs::create_dir_all(data.join("repos")).ok();
            std::fs::create_dir_all(data.join("docs")).ok();
            let db = sled::open(data.join("sled")).expect("не удалось открыть SLED базу");
            app.manage(state::AppState::new(db, data));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::fetch_models,
            commands::list_repos,
            commands::get_docs,
            commands::delete_repo,
            commands::start_generate,
            commands::start_generate_local,
            commands::cancel_generate,
            commands::chat,
            commands::generate_section,
            commands::export_xdp,
            commands::export_zip,
            commands::import_xdp,
            commands::pick_local_folder,
            commands::send_feedback
        ])
        .run(tauri::generate_context!())
        .expect("ошибка запуска tauri");
}
