//! The installer window: one small WebView2 page in the app's palette
//! (`ui/index.html`, embedded). The steps in `win.rs` run on a worker thread
//! and report progress as events; the page only shows them and sends back
//! start, close, minimise and drag.
use crate::win::{self, Event as Progress, Mode};
use serde_json::json;
use std::{borrow::Cow, process::ExitCode, sync::Mutex};
use tao::{
    dpi::{LogicalSize, PhysicalPosition},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    platform::{
        run_return::EventLoopExtRunReturn,
        windows::{IconExtWindows, WindowBuilderExtWindows, WindowExtWindows},
    },
    window::{Icon, WindowBuilder},
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_YESNO, MessageBoxW,
};
use wry::{
    WebContext, WebViewBuilder, WebViewBuilderExtWindows,
    http::{Request, Response, header::CONTENT_TYPE},
};

const TITLE: &str = "Установка Iolit";
/// The icon `build.rs` embeds (IDI_APPLICATION id).
const ICON_ID: u16 = 32512;

static FILES: [(&str, &str, &[u8]); 5] = [
    (
        "/",
        "text/html; charset=utf-8",
        include_bytes!("../ui/index.html"),
    ),
    (
        "/brand.svg",
        "image/svg+xml",
        include_bytes!("../../../resources/icon-dark.svg"),
    ),
    (
        "/fonts/FixelText-Regular.woff2",
        "font/woff2",
        include_bytes!("../../../ui/src/assets/fonts/FixelText-Regular.woff2"),
    ),
    (
        "/fonts/FixelText-Medium.woff2",
        "font/woff2",
        include_bytes!("../../../ui/src/assets/fonts/FixelText-Medium.woff2"),
    ),
    (
        "/fonts/FixelDisplay-SemiBold.woff2",
        "font/woff2",
        include_bytes!("../../../ui/src/assets/fonts/FixelDisplay-SemiBold.woff2"),
    ),
];

enum UserEvent {
    Script(String),
    Start,
    Finished(bool),
    Close,
    Minimize,
    Drag,
}

pub fn run(mode: Mode, payload: &'static [u8]) -> ExitCode {
    if !win::webview2_installed() {
        return without_webview2();
    }
    match window(mode, payload) {
        Ok(code) => code,
        Err(e) => {
            message(&format!("Не удалось открыть окно установщика: {e:#}"));
            ExitCode::FAILURE
        }
    }
}

/// A plain Windows message box for the cases the page cannot show.
pub fn message(text: &str) {
    show(text, MB_OK | MB_ICONERROR);
}

fn show(text: &str, style: u32) -> i32 {
    let text = wide(text);
    let title = wide(TITLE);
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), style) }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Windows 11 and updated Windows 10 have WebView2; stripped builds may not.
/// The app needs it too, so the user installs it once from Microsoft.
fn without_webview2() -> ExitCode {
    let answer = show(
        "Для Iolit нужен компонент Windows «Microsoft Edge WebView2», а в этой \
         системе его нет.\n\nОткрыть страницу загрузки Microsoft? После установки \
         компонента запустите этот установщик снова.",
        MB_YESNO | MB_ICONINFORMATION,
    );
    if answer == IDYES
        && let Err(e) = win::open_webview2_download()
    {
        message(&format!(
            "Не удалось открыть браузер ({e:#}). Адрес: {}",
            win::WEBVIEW2_DOWNLOAD
        ));
    }
    ExitCode::FAILURE
}

fn window(mode: Mode, payload: &'static [u8]) -> anyhow::Result<ExitCode> {
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title(TITLE)
        .with_inner_size(LogicalSize::new(552.0, 372.0))
        .with_resizable(false)
        .with_maximizable(false)
        .with_decorations(false)
        .with_undecorated_shadow(true)
        .with_visible(false)
        .with_window_icon(Icon::from_resource(ICON_ID, None).ok())
        .build(&event_loop)?;
    if let Some(monitor) = window.current_monitor() {
        let (screen, origin, size) = (monitor.size(), monitor.position(), window.outer_size());
        window.set_outer_position(PhysicalPosition::new(
            origin.x + (screen.width as i32 - size.width as i32) / 2,
            origin.y + (screen.height as i32 - size.height as i32) / 2,
        ));
    }
    round_corners(window.hwnd());

    // WebView2 would otherwise keep its profile next to the exe (Downloads).
    let profile = std::env::temp_dir().join("ZapretDesktopSetup.WebView2");
    let mut context = WebContext::new(Some(profile.clone()));
    let setup = json!({
        "mode": match mode {
            Mode::Install => "install",
            Mode::Uninstall => "uninstall",
            Mode::StopDriver => "stop",
        },
        "version": env!("CARGO_PKG_VERSION"),
        "data": win::data_dir().map(|p| p.display().to_string()).unwrap_or_default(),
    });
    let ipc = proxy.clone();
    let webview = WebViewBuilder::new_with_web_context(&mut context)
        .with_custom_protocol("zsetup".into(), |_, request| serve(request.uri().path()))
        .with_url("http://zsetup.localhost/")
        .with_initialization_script(format!("window.SETUP = {setup};"))
        .with_ipc_handler(move |request: Request<String>| {
            let event = match request.body().as_str() {
                "start" => UserEvent::Start,
                "close" => UserEvent::Close,
                "minimize" => UserEvent::Minimize,
                "drag" => UserEvent::Drag,
                _ => return,
            };
            let _ = ipc.send_event(event);
        })
        .with_background_color((0, 0, 0, 255))
        .with_devtools(false)
        .with_browser_accelerator_keys(false)
        .with_default_context_menus(false)
        .build(&window)?;
    window.set_visible(true);

    let events = Mutex::new(proxy.clone());
    win::set_reporter(Box::new(move |event| {
        if let Some(js) = script(event)
            && let Ok(proxy) = events.lock()
        {
            let _ = proxy.send_event(UserEvent::Script(js));
        }
    }));

    let mut running = false;
    let mut succeeded = false;
    event_loop.run_return(|event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            // Alt+F4 and the close button wait until the steps finish: a
            // half-copied install folder is worse than a short wait.
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            }
            | Event::UserEvent(UserEvent::Close)
                if !running =>
            {
                *flow = ControlFlow::Exit
            }
            Event::UserEvent(UserEvent::Script(js)) => {
                let _ = webview.evaluate_script(&js);
            }
            Event::UserEvent(UserEvent::Start) if !running => {
                running = true;
                start(mode, payload, proxy.clone());
            }
            Event::UserEvent(UserEvent::Finished(ok)) => {
                running = false;
                succeeded = ok;
            }
            Event::UserEvent(UserEvent::Minimize) => window.set_minimized(true),
            Event::UserEvent(UserEvent::Drag) => {
                let _ = window.drag_window();
            }
            _ => {}
        }
    });
    drop(webview);
    drop(context);
    let _ = std::fs::remove_dir_all(&profile);
    Ok(if succeeded {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn start(mode: Mode, payload: &'static [u8], proxy: EventLoopProxy<UserEvent>) {
    std::thread::spawn(move || {
        let result = win::run(mode, payload, win::After::OpenApp);
        let done = match &result {
            Ok(()) => json!({ "type": "done", "ok": true }),
            Err(e) => json!({ "type": "done", "ok": false, "error": format!("{e:#}") }),
        };
        let _ = proxy.send_event(UserEvent::Script(format!("zsetup({done})")));
        let _ = proxy.send_event(UserEvent::Finished(result.is_ok()));
    });
}

fn script(event: Progress) -> Option<String> {
    let value = match event {
        Progress::Plan(total) => json!({ "type": "plan", "total": total }),
        Progress::Step(name) => json!({ "type": "step", "name": name }),
        Progress::StepDone { ok, notes, .. } => {
            json!({ "type": "step_done", "ok": ok, "notes": notes })
        }
        Progress::Note(text) => json!({ "type": "note", "text": text }),
        Progress::Info(_) => return None,
    };
    Some(format!("zsetup({value})"))
}

fn serve(path: &str) -> Response<Cow<'static, [u8]>> {
    let response = Response::builder()
        .header("Cache-Control", "no-store")
        .header("X-Content-Type-Options", "nosniff");
    match FILES.iter().find(|(name, ..)| *name == path) {
        Some((_, kind, body)) => response
            .header(CONTENT_TYPE, *kind)
            .body(Cow::Borrowed(*body)),
        None => response.status(404).body(Cow::Borrowed(&[][..])),
    }
    .expect("static response")
}

/// Windows 11 rounds framed windows only; ask for the same on ours.
/// Windows 10 ignores the attribute.
fn round_corners(hwnd: isize) {
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
    };
    let preference = DWMWCP_ROUND;
    unsafe {
        DwmSetWindowAttribute(
            hwnd as _,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &preference as *const _ as *const _,
            size_of_val(&preference) as u32,
        );
    }
}
