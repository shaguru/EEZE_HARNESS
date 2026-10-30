mod claude;
mod commands;
mod netenv;
mod portable_paths;
mod protocol;
mod rpc_transport;
mod scaffold;
mod state;
mod tools;
use commands::AppState;
use portable_paths::PortablePaths;
use std::sync::{Arc, Mutex};
use tauri::Manager;

fn startup_error(text: &str) {
    rfd::MessageDialog::new()
        .set_title("RustPortableHarness")
        .set_description(text)
        .set_level(rfd::MessageLevel::Error)
        .show();
}

pub fn run() {
    // Checked before any window exists so the message is about the real problem.
    let paths = match PortablePaths::discover() {
        Ok(p) => p,
        Err(e) => return startup_error(&e),
    };
    let instance = match paths.lock_instance() {
        Ok(f) => Some(f),
        Err(e) if e.contains("already open") => return startup_error(&e),
        Err(_) => None,
    };
    netenv::clean_temp(&paths.data.join("temp"), 3);
    let config = match paths.load() {
        Ok(c) => c,
        Err(e) => return startup_error(&e),
    };
    if config.migrated_permissions {
        let _ = paths.save(&config);
    }
    let _ = std::fs::create_dir_all(paths.root.join("projects"));
    let result = tauri::Builder::default()
        .setup(move |app| {
            let mut runtime = state::Runtime::new(config.clone(), paths.root.to_string_lossy().into());
            runtime.view.notices = netenv::startup_notices(&paths.root, &config);
            runtime.view.engines = commands::engines(&paths, &config);
            commands::refresh_env_info(&paths, &mut runtime);
            if config.migrated_permissions {
                runtime.view.notices.push(netenv::Notice {
                    id: "permissions".into(),
                    level: "info".into(),
                    title: "The AI can now save files in your project".into(),
                    text: "\"Ask before changes\" made Codex refuse every file change, so the app now uses \"Edit files in this project\". The AI still stays inside the project folder. Change it any time with the dropdown on the message box.".into(),
                });
            }
            let shared = Arc::new(Mutex::new(runtime));
            let web_data = paths.data.join("webview2");
            app.manage(AppState {
                paths: paths.clone(),
                shared,
                operation: tokio::sync::Mutex::new(()),
                _instance: instance,
            });
            // Lab monitors are often 1366×768 at 125% scaling; start maximized when the screen is small.
            let small = app
                .primary_monitor()
                .ok()
                .flatten()
                .map(|m| {
                    let size = m.size().to_logical::<f64>(m.scale_factor());
                    size.width < 1300.0 || size.height < 860.0
                })
                .unwrap_or(false);
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("RustPortableHarness")
            .inner_size(1240.0, 820.0)
            .min_inner_size(760.0, 500.0)
            .maximized(small)
            .data_directory(web_data)
            .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::snapshot,
            commands::connect,
            commands::disconnect,
            commands::set_provider,
            commands::select_project,
            commands::reconnect_project,
            commands::forget_project,
            commands::refresh_threads,
            commands::open_thread,
            commands::archive_thread,
            commands::rename_thread,
            commands::send_turn,
            commands::interrupt,
            commands::decide,
            commands::save_preferences,
            commands::set_ca_certificate,
            commands::choose_binary,
            commands::auth,
            commands::add_account,
            commands::switch_account,
            commands::rename_account,
            commands::remove_account,
            commands::refresh_usage,
            commands::diagnostics,
            commands::wipe_account,
            commands::export_diagnostics,
            commands::open_external,
            commands::open_project_folder,
            commands::install_tools,
            commands::remove_tool,
            commands::choose_folder,
            commands::create_project,
            commands::run_project,
            commands::open_terminal,
            commands::restart_ai,
            commands::export_classroom_copy
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
                    if let Some(session) = r.claude.take() {
                        session.stop_now();
                    }
                    if let Some(login) = r.claude_login.take() {
                        login.cancel();
                    }
                    r.pending.clear();
                }
            }
        }),
        Err(e) => startup_error(&format!("{e}\n\nThis app needs the Microsoft Edge WebView2 Runtime, which is normally part of Windows 10/11. It does not install anything or ask for administrator access. If WebView2 is missing, ask IT for it. Also check that the app folder is writable. See README-PORTABLE.md.")),
    }
}
