mod commands;
pub mod defaults;
mod directory_cache;
mod editor;
mod error;
mod ftp;
mod hostkey;
pub mod list_parser;
mod local;
mod local_path;
mod logs;
pub mod models;
mod proxy;
mod remote;
pub mod remote_path;
mod secrets;
mod server_path;
mod session;
mod sftp;
mod site_xml;
mod state;
mod store;
mod transfers;

pub use error::AppError;

use editor::EditManager;
use logs::LogBus;
use session::SessionManager;
use state::AppState;
use std::sync::Arc;
use store::Store;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use tracing_subscriber::EnvFilter;
use transfers::TransferManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn,filezilla_rs_lib=info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();

    // Install the rustls crypto provider explicitly: FTPS may otherwise panic
    // when more than one provider feature ends up enabled by the dependency graph.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let context = tauri::generate_context!();
    // With "close to tray" enabled the app keeps running in the background, so
    // a second launch must activate the existing window instead of starting a
    // duplicate instance. The decision has to be made before the plugin list is
    // built, which means peeking at settings.json on disk.
    let single_instance = peek_minimize_to_tray(&context.config().identifier);

    let mut builder = tauri::Builder::default();
    if single_instance {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main(app);
        }));
    }
    let builder = builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if let Some(state) = window.app_handle().try_state::<AppState>() {
                    if state.store.settings().minimize_to_tray {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
            }
        })
        .setup(|app| {
            // `FILEZILLA_RS_CONFIG_DIR` keeps development / sandbox runs inside
            // the workspace instead of writing to %APPDATA%.
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(std::path::Path::to_path_buf));
            let app_config = app.path().app_config_dir().ok();
            let policy = defaults::load(exe_dir.as_deref(), app_config.as_deref());
            let config_dir = std::env::var_os("FILEZILLA_RS_CONFIG_DIR")
                .map(std::path::PathBuf::from)
                .or_else(|| policy.config_dir.as_deref().map(std::path::PathBuf::from))
                .or(app_config)
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            if let Err(err) = std::fs::create_dir_all(&config_dir) {
                if !config_dir.is_dir() {
                    return Err(err.into());
                }
            }
            let store = Arc::new(Store::load_with_policy(
                config_dir,
                policy.kiosk_mode,
                policy.read_only,
            ));
            let logs = Arc::new(LogBus::new(app.handle().clone()));
            if !policy.read_only && store.snapshot().sites.is_empty() {
                if let Some(source) = policy.loaded_from.clone() {
                    match crate::site_xml::import_file(&store, std::path::Path::new(&source)) {
                        Ok(outcome) if outcome.imported > 0 => logs.info(
                            "sites",
                            format!(
                                "Seeded {} default site(s) from fzdefaults.xml",
                                outcome.imported
                            ),
                        ),
                        _ => {}
                    }
                }
            }
            hostkey::init(app.handle().clone());
            let sessions = Arc::new(SessionManager::new(store.clone()));
            let transfers = TransferManager::new(
                app.handle().clone(),
                sessions.clone(),
                store.clone(),
                logs.clone(),
            );

            logs.info(
                "app",
                format!(
                    "FileZilla Modern {} starting (Tauri {})",
                    env!("CARGO_PKG_VERSION"),
                    tauri::VERSION
                ),
            );

            let edits = EditManager::new(app.handle().clone(), sessions.clone(), store.clone());

            app.manage(AppState {
                store,
                sessions,
                transfers,
                logs,
                policy: Arc::new(policy),
                edits,
            });

            let _ = create_tray(app.handle());

            if let Some(window) = app.get_webview_window("main") {
                #[cfg(target_os = "windows")]
                {
                    let dark = matches!(window.theme(), Ok(tauri::Theme::Dark));
                    if window_vibrancy::apply_mica(&window, Some(dark)).is_err() {
                        let _ = window_vibrancy::apply_acrylic(&window, Some((22, 24, 29, 200)));
                    }
                }
                window.show()?;
                let _ = window.set_focus();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::managed_policy,
            commands::settings_get,
            commands::settings_set,
            commands::sites_get,
            commands::site_save,
            commands::site_delete,
            commands::site_duplicate,
            commands::sites_import,
            commands::sites_detect_auto,
            commands::sites_import_auto,
            commands::sites_export,
            commands::group_create,
            commands::group_rename,
            commands::group_delete,
            commands::connect,
            commands::disconnect,
            commands::remote_list,
            commands::remote_stat,
            commands::remote_mkdir,
            commands::remote_rename,
            commands::remote_chmod,
            commands::remote_delete,
            commands::local_list,
            commands::local_mkdir,
            commands::local_rename,
            commands::local_delete,
            commands::remote_edit_start,
            commands::remote_edit_stop,
            commands::ssh_hostkey_resolve,
            commands::search_remote,
            commands::enqueue,
            commands::transfers_list,
            commands::transfer_summary,
            commands::transfer_action,
            commands::transfer_clear,
            commands::logs_history,
            commands::logs_clear,
            commands::known_hosts_list,
            commands::known_host_forget,
            commands::log_client,
            commands::apply_window_theme,
        ]);

    let app = builder
        .build(context)
        .expect("failed to build FileZilla Modern");

    app.run(|handle, event| {
        if let tauri::RunEvent::Exit = event {
            let state = handle.state::<AppState>();
            tauri::async_runtime::block_on(async {
                state.edits.stop_all();
                state.sessions.close_all().await;
            });
        }
    });
}
/// Directory the store will use. Resolved before the Tauri app exists so the
/// single-instance decision can read the persisted settings.
fn preflight_config_dir(identifier: &str) -> std::path::PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf));
    let app_config = default_app_config_dir(identifier);
    let policy = defaults::load(exe_dir.as_deref(), app_config.as_deref());
    std::env::var_os("FILEZILLA_RS_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| policy.config_dir.as_deref().map(std::path::PathBuf::from))
        .or(app_config)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

fn default_app_config_dir(identifier: &str) -> Option<std::path::PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(|base| std::path::PathBuf::from(base).join(identifier))
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| {
            std::path::PathBuf::from(home)
                .join("Library/Application Support")
                .join(identifier)
        })
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| std::path::PathBuf::from(home).join(".config"))
            })
            .map(|base| base.join(identifier))
    }
}

fn peek_minimize_to_tray(identifier: &str) -> bool {
    let path = preflight_config_dir(identifier).join("settings.json");
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    serde_json::from_slice::<models::AppSettings>(&bytes)
        .map(|settings| settings.minimize_to_tray)
        .unwrap_or(false)
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let Some(icon) = app.default_window_icon().cloned() else {
        return Ok(());
    };
    let show = MenuItem::with_id(app, "show", "Show FileZilla Modern", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("FileZilla Modern")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}
