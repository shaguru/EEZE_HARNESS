mod commands;
mod portable_paths;
mod protocol;
mod rpc_transport;
mod state;
use commands::AppState;
use portable_paths::PortablePaths;
use std::sync::{Arc, Mutex};
use tauri::Manager;
pub fn run() {
    let result = tauri::Builder::default()
        .setup(|app| {
            let paths = PortablePaths::discover()?;
            let config = paths.load()?;
            let shared = Arc::new(Mutex::new(state::Runtime::new(
                config,
                paths.root.to_string_lossy().into(),
            )));
            let web_data = paths.data.join("webview2");
            app.manage(AppState {
                paths,
                shared,
                operation: tokio::sync::Mutex::new(()),
            });
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("RustPortableHarness")
            .inner_size(1200.0, 800.0)
            .min_inner_size(900.0, 600.0)
            .data_directory(web_data)
            .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::snapshot,
            commands::connect,
            commands::disconnect,
            commands::select_project,
            commands::reconnect_project,
            commands::refresh_threads,
            commands::open_thread,
            commands::archive_thread,
            commands::rename_thread,
            commands::send_turn,
            commands::interrupt,
            commands::decide,
            commands::save_preferences,
            commands::choose_binary,
            commands::auth,
            commands::diagnostics,
            commands::wipe_account,
            commands::export_diagnostics,
            commands::open_external
        ])
        .build(tauri::generate_context!());
    match result {
        Ok(app) => app.run(|handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(state) = handle.try_state::<AppState>() {
                    let mut r = state.shared.lock().unwrap();
                    if let Some(rpc) = r.rpc.take() {
                        rpc.stop_now();
                    }
                    r.pending.clear();
                }
            }
        }),
        Err(e) => {
            rfd::MessageDialog::new().set_title("RustPortableHarness could not start").set_description(format!("{e}\n\nThis build requires Microsoft Edge WebView2 Runtime already installed. It does not install a runtime or request administrator access. If WebView2 is unavailable, ask IT for an approved runtime. Check that the app's data folder is writable. See README-PORTABLE.md.")).set_level(rfd::MessageLevel::Error).show();
        }
    }
}
