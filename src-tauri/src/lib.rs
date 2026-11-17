//! Unprivileged desktop shell: one daemon request per action, plus native dialogs.
mod report;

use std::path::PathBuf;
use tauri::Manager;
use zapret_core::{ipc::request_command, *};

type ApiResult<T> = Result<T, String>;
fn msg(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
async fn snapshot() -> ApiResult<Snapshot> {
    // Presentation of an unavailable service keeps the installation UI usable.
    Ok(request_command(DaemonCommand::Snapshot)
        .await
        .unwrap_or_else(|e| Snapshot {
            version: VERSION.into(),
            service: ServiceStatus::unavailable(e.to_string()),
            strategies: vec![],
            operation: Operation::default(),
            bundle: BundleStatus::default(),
            health: Default::default(),
            app: Default::default(),
            telegram: Default::default(),
        }))
}
#[tauri::command]
async fn enable() -> ApiResult<()> {
    request_command(DaemonCommand::Enable).await.map_err(msg)
}
#[tauri::command]
async fn disable() -> ApiResult<()> {
    request_command(DaemonCommand::Disable).await.map_err(msg)
}
#[tauri::command(rename_all = "snake_case")]
async fn set_strategy(strategy_id: String) -> ApiResult<()> {
    request_command(DaemonCommand::SetStrategy { strategy_id })
        .await
        .map_err(msg)
}
#[tauri::command]
async fn diagnose() -> ApiResult<()> {
    request_command(DaemonCommand::Diagnose).await.map_err(msg)
}
#[tauri::command]
async fn select_strategy() -> ApiResult<()> {
    request_command(DaemonCommand::SelectStrategy)
        .await
        .map_err(msg)
}
#[tauri::command]
async fn repair_strategy() -> ApiResult<()> {
    request_command(DaemonCommand::RepairStrategy)
        .await
        .map_err(msg)
}
#[tauri::command]
async fn update_app() -> ApiResult<()> {
    request_command(DaemonCommand::UpdateApp).await.map_err(msg)
}
#[tauri::command]
async fn cancel_operation() -> ApiResult<()> {
    request_command(DaemonCommand::CancelOperation)
        .await
        .map_err(msg)
}
#[tauri::command]
async fn add_domain(domain: String) -> ApiResult<()> {
    request_command(DaemonCommand::AddDomain { domain })
        .await
        .map_err(msg)
}
#[tauri::command]
async fn set_secure_dns(enabled: bool) -> ApiResult<()> {
    request_command(DaemonCommand::SetSecureDns { enabled })
        .await
        .map_err(msg)
}
#[tauri::command]
async fn set_telegram_proxy(enabled: bool) -> ApiResult<()> {
    request_command(DaemonCommand::SetTelegramProxy { enabled })
        .await
        .map_err(msg)
}
/// Hands the service's own `tg://proxy` link to the system, which opens
/// Telegram Desktop on its "add proxy" prompt. The link is asked from the
/// service here and checked, never taken from the window.
#[tauri::command]
async fn open_telegram() -> ApiResult<()> {
    let snapshot: Snapshot = request_command(DaemonCommand::Snapshot)
        .await
        .map_err(msg)?;
    let link = snapshot
        .telegram
        .own_link()
        .ok_or("Сначала включите прокси для Telegram")?;
    #[cfg(windows)]
    let mut command = {
        let mut command = tokio::process::Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(not(windows))]
    let mut command = tokio::process::Command::new("/usr/bin/open");
    let status = command.arg(link).status().await.map_err(msg)?;
    if !status.success() {
        return Err(
            "Telegram не открылся. Скопируйте ссылку и откройте её в Telegram вручную.".into(),
        );
    }
    Ok(())
}
#[tauri::command]
async fn remove_domain(domain: String) -> ApiResult<()> {
    request_command(DaemonCommand::RemoveDomain { domain })
        .await
        .map_err(msg)
}
#[tauri::command]
async fn domains() -> ApiResult<Vec<String>> {
    request_command(DaemonCommand::Domains).await.map_err(msg)
}
#[tauri::command]
async fn doctor() -> ApiResult<DoctorResult> {
    request_command(DaemonCommand::Doctor).await.map_err(msg)
}
#[tauri::command]
async fn logs() -> ApiResult<Vec<String>> {
    request_command(DaemonCommand::Logs).await.map_err(msg)
}
#[tauri::command]
async fn report() -> ApiResult<Report> {
    request_command(DaemonCommand::Report).await.map_err(msg)
}
#[tauri::command]
async fn export_report(path: PathBuf) -> ApiResult<()> {
    if path.extension().and_then(|e| e.to_str()) != Some("txt") {
        return Err("Отчёт нужно сохранить как .txt".into());
    }
    tokio::fs::write(path, report::build().await)
        .await
        .map_err(msg)
}

#[tauri::command]
async fn install_service(app: tauri::AppHandle) -> ApiResult<()> {
    #[cfg(target_os = "macos")]
    {
        let pkg = app
            .path()
            .resource_dir()
            .map_err(msg)?
            .join("runtime/ZapretService.pkg");
        if !pkg.is_file() {
            return Err(
                "Установщик службы не найден. Переустановите приложение из полного пакета.".into(),
            );
        }
        let status = tokio::process::Command::new("/usr/bin/open")
            .arg(pkg)
            .status()
            .await
            .map_err(msg)?;
        if !status.success() {
            return Err("Не удалось открыть установщик".into());
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("Запустите установщик Iolit от имени администратора, чтобы установить службу.".into())
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            snapshot,
            enable,
            disable,
            set_strategy,
            diagnose,
            select_strategy,
            repair_strategy,
            update_app,
            cancel_operation,
            add_domain,
            remove_domain,
            set_secure_dns,
            set_telegram_proxy,
            open_telegram,
            domains,
            doctor,
            logs,
            report,
            install_service,
            export_report
        ])
        .setup(|app| {
            use tauri::{
                menu::{Menu, MenuItem},
                tray::TrayIconBuilder,
            };
            let show = MenuItem::with_id(app, "show", "Открыть Iolit", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Выйти из приложения", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray =
                // A fixed id lets the window swap the tray icon for the user's one.
                TrayIconBuilder::with_id("main")
                    .menu(&menu)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "show" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        "quit" => app.exit(0),
                        _ => {}
                    });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Не удалось запустить Iolit");
}
