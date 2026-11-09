use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Manager, State};
use tokio::sync::Mutex;
use zapret_core::{ipc::request, *};

#[derive(Default)]
struct AppState {
    operation: Mutex<Operation>,
    cancelled: AtomicBool,
    gate: Arc<Mutex<()>>,
    recovery_path: Mutex<Option<PathBuf>>,
}
type Shared = Arc<AppState>;
type ApiResult<T> = Result<T, String>;
fn msg(e: impl std::fmt::Display) -> String {
    e.to_string()
}

async fn service_status() -> anyhow::Result<ServiceStatus> {
    let s = request("status", json!({})).await?;
    #[cfg(windows)]
    {
        return Ok(serde_json::from_value(s)?);
    }
    #[cfg(not(windows))]
    {
        let mut warnings: Vec<String> =
            serde_json::from_value(s["warnings"].clone()).unwrap_or_default();
        let running = s["running"].as_bool().unwrap_or(false);
        if running && s["transport"] != "divert" {
            warnings.push("Активен ограниченный транспорт. UDP не поддерживается.".into());
        }
        if let Some(unsupported) = s["unsupported"].as_array() {
            for op in unsupported {
                warnings.push(format!(
                    "Стратегия не поддерживает операцию: {}",
                    op.as_str().unwrap_or("unknown")
                ));
            }
        }
        Ok(ServiceStatus {
            installed: true,
            running,
            platform: "macos".into(),
            engine: "PF / BPF".into(),
            strategy: s["strategy"].as_str().unwrap_or_default().into(),
            uptime_secs: s["datapath_uptime_sec"].as_f64().unwrap_or(0.) as u64,
            capabilities: Capabilities {
                tcp: s["caps"]["segment"].as_bool().unwrap_or(false),
                udp: s["caps"]["udp"].as_bool().unwrap_or(false),
                ipv6: s["caps"]["ipv6_exthdr"].as_bool().unwrap_or(false),
                packet_injection: s["caps"]["inject"].as_bool().unwrap_or(false),
            },
            warnings,
            packets: s["stats"]["pkts_in"].as_u64().unwrap_or(0),
            modified_packets: s["stats"]["desyncs"].as_u64().unwrap_or(0),
            errors: s["stats"]["errors"].as_u64().unwrap_or(0),
        })
    }
}

async fn strategies() -> anyhow::Result<Vec<Strategy>> {
    let data = request("list", json!({})).await?;
    #[cfg(windows)]
    {
        Ok(serde_json::from_value(data)?)
    }
    #[cfg(not(windows))]
    {
        Ok(data["strategies"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .map(|s| Strategy {
                        id: s["name"].as_str().unwrap_or_default().into(),
                        name: s["name"].as_str().unwrap_or_default().into(),
                        description: s["summary"].as_str().unwrap_or_default().into(),
                        supported: s["unsupported"].as_array().is_none_or(|a| a.is_empty()),
                        arguments: vec![],
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
}

#[tauri::command]
async fn snapshot(state: State<'_, Shared>) -> ApiResult<Snapshot> {
    let service = service_status()
        .await
        .unwrap_or_else(|e| ServiceStatus::unavailable(e.to_string()));
    let strategies = if service.installed {
        strategies().await.unwrap_or_default()
    } else {
        vec![]
    };
    Ok(Snapshot {
        version: VERSION.into(),
        service,
        strategies,
        operation: state.operation.lock().await.clone(),
    })
}

#[tauri::command]
async fn control(
    action: String,
    strategy_id: Option<String>,
    state: State<'_, Shared>,
) -> ApiResult<()> {
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| "Дождитесь завершения проверки или отмените её".to_string())?;
    match action.as_str() {
        "enable" => {
            request("start", json!({"transport":"divert"}))
                .await
                .map_err(msg)?;
        }
        "disable" => {
            request("stop", json!({})).await.map_err(msg)?;
        }
        "strategy" => {
            let id = strategy_id.ok_or("Стратегия не выбрана")?;
            if !strategy::valid_id(&id) {
                return Err("Некорректное имя стратегии".into());
            }
            request("use", json!({"name":id})).await.map_err(msg)?;
        }
        _ => return Err("Неизвестная команда".into()),
    }
    Ok(())
}

#[tauri::command]
async fn diagnose(state: State<'_, Shared>) -> ApiResult<()> {
    launch_operation(state.inner().clone(), false).await
}
#[tauri::command]
async fn select_strategy(state: State<'_, Shared>) -> ApiResult<()> {
    launch_operation(state.inner().clone(), true).await
}

async fn launch_operation(state: Shared, selection: bool) -> ApiResult<()> {
    let gate = state
        .gate
        .clone()
        .try_lock_owned()
        .map_err(|_| "Проверка уже выполняется".to_string())?;
    state.cancelled.store(false, Ordering::Release);
    *state.operation.lock().await = Operation {
        busy: true,
        stage: if selection {
            "Подготавливаем подбор"
        } else {
            "Проверяем соединения"
        }
        .into(),
        ..Default::default()
    };
    tauri::async_runtime::spawn(async move {
        let _gate = gate;
        let result = if selection {
            auto_select(&state).await
        } else {
            let results = zapret_core::probe::run().await;
            state.operation.lock().await.results = results;
            Ok(())
        };
        let mut operation = state.operation.lock().await;
        operation.busy = false;
        operation.stage = if state.cancelled.load(Ordering::Acquire) {
            "Проверка отменена"
        } else if result.is_err() {
            "Проверка завершилась с ошибкой"
        } else {
            "Проверка завершена"
        }
        .into();
        operation.error = result.err().map(|e| format!("{e:#}"));
    });
    Ok(())
}

async fn wait_ready() -> anyhow::Result<()> {
    for _ in 0..30 {
        if service_status().await?.running {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    anyhow::bail!("Движок не запустился")
}

async fn restore_service(original: &ServiceStatus) -> anyhow::Result<()> {
    request("stop", json!({})).await?;
    if !original.strategy.is_empty() {
        request("use", json!({"name":original.strategy})).await?;
    }
    if original.running {
        request("start", json!({"transport":"divert"})).await?;
    }
    Ok(())
}

async fn auto_select(state: &Shared) -> anyhow::Result<()> {
    let original = service_status().await?;
    let candidates: Vec<_> = strategies()
        .await?
        .into_iter()
        .filter(|s| s.supported)
        .collect();
    if candidates.is_empty() {
        anyhow::bail!("Нет совместимых стратегий");
    }
    let recovery_path = state
        .recovery_path
        .lock()
        .await
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Каталог восстановления недоступен"))?;
    tokio::fs::write(&recovery_path, serde_json::to_vec(&original)?).await?;
    state.operation.lock().await.total = candidates.len();
    // Always roll back on cancellation, failed probes, or a service/IPC error.
    let outcome = async {
        request("stop", json!({})).await?;
        state.operation.lock().await.stage = "Проверяем доступность без обхода".into();
        let baseline = zapret_core::probe::run().await;
        let all_accessible = baseline.iter().all(|r| r.ok);
        state.operation.lock().await.results = baseline;
        if all_accessible {
            state.operation.lock().await.notice = Some(
                "Все проверенные соединения доступны без обхода. Исходная настройка сохранена."
                    .into(),
            );
            return Ok(None);
        }
        for (index, s) in candidates.iter().enumerate() {
            if state.cancelled.load(Ordering::Acquire) {
                return Ok(None);
            }
            {
                let mut op = state.operation.lock().await;
                op.current = index + 1;
                op.stage = format!("Проверяем {}", s.name);
            }
            request("stop", json!({})).await?;
            request("use", json!({"name":s.id})).await?;
            request("start", json!({"transport":"divert"})).await?;
            wait_ready().await?;
            // New clients and sockets per round prevent connection reuse between candidates.
            let first = zapret_core::probe::run().await;
            let first_pass = first.iter().all(|r| r.ok);
            state.operation.lock().await.results = first;
            if first_pass && !state.cancelled.load(Ordering::Acquire) {
                let confirmation = zapret_core::probe::run().await;
                let passed = confirmation.iter().all(|r| r.ok);
                state.operation.lock().await.results = confirmation;
                if passed {
                    return Ok::<_, anyhow::Error>(Some(s.id.clone()));
                }
            }
        }
        Ok(None)
    }
    .await;
    match outcome {
        Ok(Some(winner)) if !state.cancelled.load(Ordering::Acquire) => {
            state.operation.lock().await.winner = Some(winner);
            tokio::fs::remove_file(&recovery_path).await?;
            Ok(())
        }
        other => {
            restore_service(&original).await.map_err(|e| {
                anyhow::anyhow!("Не удалось восстановить исходную настройку: {e:#}")
            })?;
            tokio::fs::remove_file(&recovery_path).await?;
            match other {
                Err(e) => Err(e),
                _ if state.cancelled.load(Ordering::Acquire) => Ok(()),
                _ if state.operation.lock().await.notice.is_some() => Ok(()),
                _ => anyhow::bail!(
                    "Ни одна стратегия не прошла все проверки. Исходная настройка восстановлена."
                ),
            }
        }
    }
}

#[tauri::command]
fn cancel_operation(state: State<'_, Shared>) {
    state.cancelled.store(true, Ordering::Release);
}

#[tauri::command]
async fn add_domain(domain: String, state: State<'_, Shared>) -> ApiResult<()> {
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| "Дождитесь завершения проверки".to_string())?;
    let domain = strategy::normalize_domain(&domain).map_err(msg)?;
    request("add-domain", json!({"domain":domain}))
        .await
        .map_err(msg)?;
    Ok(())
}

#[tauri::command]
async fn doctor(state: State<'_, Shared>) -> ApiResult<Value> {
    let _guard = state
        .gate
        .try_lock()
        .map_err(|_| "Дождитесь завершения проверки".to_string())?;
    request("doctor", json!({})).await.map_err(msg)
}

#[tauri::command]
async fn logs() -> ApiResult<Vec<String>> {
    let data = request("logtail", json!({"lines":"80"}))
        .await
        .map_err(msg)?;
    Ok(serde_json::from_value(data["lines"].clone()).unwrap_or_default())
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
        Err(
            "Запустите установщик Zapret Desktop от имени администратора, чтобы установить службу."
                .into(),
        )
    }
}

#[tauri::command]
async fn export_report(path: PathBuf, state: State<'_, Shared>) -> ApiResult<()> {
    // Raw daemon logs can contain IP addresses: they are intentionally excluded.
    let service = service_status()
        .await
        .unwrap_or_else(|_| ServiceStatus::unavailable("Служба недоступна".into()));
    let operation = state.operation.lock().await.clone();
    let report = json!({"app_version":VERSION,"platform":service.platform,"engine":service.engine,"running":service.running,"strategy":service.strategy,"capabilities":service.capabilities,"packets":service.packets,"errors":service.errors,"checks":operation.results.iter().map(|r|json!({"service":r.id,"protocol":r.protocol,"ok":r.ok,"latency_ms":r.latency_ms})).collect::<Vec<_>>()});
    // The path comes from the native save dialog; limit the write to a report file.
    if path.extension().and_then(|e| e.to_str()) != Some("json") {
        return Err("Отчёт нужно сохранить как .json".into());
    }
    tokio::fs::write(path, serde_json::to_vec_pretty(&report).map_err(msg)?)
        .await
        .map_err(msg)
}

pub fn run() {
    let state = Arc::new(AppState::default());
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            snapshot,
            control,
            diagnose,
            select_strategy,
            cancel_operation,
            add_domain,
            doctor,
            logs,
            install_service,
            export_report
        ])
        .setup(|app| {
            let app_data=app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data)?;
            let recovery_path=app_data.join("selection-recovery.json");
            let state=app.state::<Shared>().inner().clone();
            let task_state=state.clone();
            tauri::async_runtime::block_on(async { *state.recovery_path.lock().await=Some(recovery_path.clone()); });
            if recovery_path.exists() {
                tauri::async_runtime::spawn(async move {
                    let _gate=task_state.gate.lock().await;
                    *task_state.operation.lock().await=Operation{busy:true,stage:"Восстанавливаем настройку после прерванного подбора".into(),..Default::default()};
                    let result=async {
                        let saved:ServiceStatus=serde_json::from_slice(&tokio::fs::read(&recovery_path).await?)?;
                        restore_service(&saved).await?;
                        tokio::fs::remove_file(recovery_path).await?;
                        Ok::<_,anyhow::Error>(())
                    }.await;
                    let mut op=task_state.operation.lock().await;
                    op.busy=false;op.stage="Восстановление настройки".into();
                    match result {Ok(())=>op.notice=Some("Предыдущая настройка восстановлена.".into()),Err(e)=>op.error=Some(format!("Не удалось автоматически восстановить настройку: {e:#}. Проверьте системную службу."))};
                });
            }
            use tauri::{
                menu::{Menu, MenuItem},
                tray::TrayIconBuilder,
            };
            let show = MenuItem::with_id(app, "show", "Открыть Zapret", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Выйти из приложения", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray =
                TrayIconBuilder::new()
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
        .expect("Не удалось запустить Zapret Desktop");
}
