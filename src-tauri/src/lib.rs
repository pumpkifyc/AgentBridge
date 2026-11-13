// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppState::default())
        .manage(commands::ReceiveState::default())
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::codex_status,
            commands::list_local_addresses,
            commands::list_threads,
            commands::list_turns,
            commands::preview_snapshot,
            commands::cancel_preview,
            commands::start_share,
            commands::share_status,
            commands::revoke_share,
            commands::cancel_share,
            commands::preview_share,
            commands::decline_share,
            commands::accept_share,
            commands::cancel_transfer,
            commands::retry_preview,
            commands::delete_staged_snapshot,
            commands::select_project_directory,
            commands::import_snapshot,
            commands::list_imports,
            commands::retry_import,
            commands::open_import,
            commands::copy_continuation_prompt,
            commands::delete_import,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub mod codex;
pub mod commands;
pub mod db;
pub mod imports;
pub mod native;
pub mod pairing;
pub mod process;
pub mod snapshot;
pub mod transfer;
