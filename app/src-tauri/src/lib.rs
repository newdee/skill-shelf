use tauri::Manager;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

/// The port the bundled server sidecar listens on (must match the frontend's
/// desktop default in src/lib/config.ts).
const SIDECAR_PORT: &str = "8765";

/// Start the bundled `skill-shelf-server` sidecar, pointing it at the app's
/// local data dir (SQLite) so the desktop app works fully offline. The
/// frontend then talks to it over HTTP at 127.0.0.1:SIDECAR_PORT.
fn start_sidecar(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;

    let (mut rx, child) = app
        .shell()
        .sidecar("skill-shelf-server")?
        .env("DATA_DIR", data_dir.to_string_lossy().to_string())
        .env("PORT", SIDECAR_PORT)
        .spawn()?;

    // Keep the child alive for the app's lifetime; the shell plugin terminates
    // sidecars on app exit.
    app.manage(std::sync::Mutex::new(Some(child)));

    // Surface sidecar logs into the app log.
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(bytes) | CommandEvent::Stderr(bytes) => {
                    log::info!("[server] {}", String::from_utf8_lossy(&bytes).trim_end());
                }
                CommandEvent::Error(err) => log::error!("[server] {err}"),
                CommandEvent::Terminated(payload) => {
                    log::warn!("[server] terminated: {payload:?}")
                }
                _ => {}
            }
        }
    });

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            start_sidecar(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
