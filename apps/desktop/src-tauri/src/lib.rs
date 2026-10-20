mod commands;
mod state;

use tauri::Manager;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    tauri::Builder::default()
        // A second launch focuses the running window instead of opening another one.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let state = state::AppState::new(app.handle().clone()).map_err(|e| e.to_string())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::settings_save,
            commands::profile_save,
            commands::profile_delete,
            commands::profile_verify,
            commands::profile_test_ssh,
            commands::profile_for_url,
            commands::github_device_start,
            commands::github_device_poll,
            commands::hosted_repos,
            commands::rules_save,
            commands::repo_add,
            commands::repo_remove,
            commands::repo_bind,
            commands::repo_overview,
            commands::repo_status,
            commands::repo_diff,
            commands::repo_guard,
            commands::repo_commit,
            commands::repo_undo_commit,
            commands::repo_discard,
            commands::repo_fix_authors,
            commands::repo_abort,
            commands::repo_fetch,
            commands::repo_pull,
            commands::repo_push,
            commands::repo_branches,
            commands::repo_switch,
            commands::repo_create_branch,
            commands::repo_log,
            commands::repo_commit_files,
            commands::repo_commit_diff,
            commands::repo_clone,
            commands::repo_init,
            commands::tools_status,
            commands::tools_lfs_install_global,
            commands::tools_winget_install,
            commands::repo_lfs_install,
            commands::open_terminal,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Gitwise");
}
