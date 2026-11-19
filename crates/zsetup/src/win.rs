//! Windows install and uninstall steps.
//!
//! Every path is fixed: the install folder comes from the Program Files known
//! folder, never from the command line or the environment. The service gets
//! a fixed command line. Nothing here runs scripts or other installers.
use crate::payload::{self, Entry, Record};
use anyhow::{Context, Result, anyhow, bail};
use std::{
    ffi::{OsStr, OsString},
    fs, io,
    os::windows::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    sync::{
        OnceLock,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_service::{
    service::{
        Service, ServiceAccess, ServiceAction, ServiceActionType, ServiceErrorControl,
        ServiceFailureActions, ServiceFailureResetPeriod, ServiceInfo, ServiceStartType,
        ServiceState, ServiceType,
    },
    service_manager::{ServiceManager, ServiceManagerAccess},
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_ALREADY_EXISTS, ERROR_SERVICE_ALREADY_RUNNING,
        ERROR_SERVICE_CANNOT_ACCEPT_CTRL, ERROR_SERVICE_DOES_NOT_EXIST,
        ERROR_SERVICE_MARKED_FOR_DELETE, ERROR_SERVICE_NOT_ACTIVE, GetLastError, HANDLE,
        INVALID_HANDLE_VALUE, LocalFree,
    },
    Security::{
        Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SE_FILE_OBJECT,
            SetNamedSecurityInfoW,
        },
        DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, GetSecurityDescriptorOwner,
        GetTokenInformation, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    },
    Storage::FileSystem::{
        CreateDirectoryW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        MOVEFILE_DELAY_UNTIL_REBOOT, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        MoveFileExW,
    },
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
        Threading::{
            CreateMutexW, GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
            QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
        },
    },
};
use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
};

/// The name people see (renamed from Zapret Desktop on 2026-11-17).
const PRODUCT: &str = "Iolit";
// The service key keeps the old name until the internal rename. The install
// and data folders are chosen by `payload::install_dir` and
// `payload::data_dir`: new installations use `Iolit`, old ones stay.
const VERSION: &str = env!("CARGO_PKG_VERSION");
const PUBLISHER: &str = "Iolit";
const SERVICE: &str = "ZapretDesktop";
const SERVICE_DISPLAY: &str = "Iolit";
const SERVICE_DESCRIPTION: &str =
    "Управление локальным обходом DPI. Приложение работает без прав администратора.";
/// Kernel driver service that winws creates through WinDivert.dll.
const DRIVER: &str = "WinDivert";
/// Driver services WinDivert.dll registers: 2.x and the older 1.4.
const DRIVERS: [&str; 2] = ["WinDivert", "WinDivert14"];
const APP_EXE: &str = "zapret-desktop.exe";
const UNINSTALLER: &str = "uninstall.exe";
const SHORTCUT: &str = "Iolit.lnk";
const STOP_SHORTCUT: &str = "Iolit — остановить WinDivert.lnk";
/// Shortcuts of the versions named Zapret Desktop (and of the NSIS installer
/// before them), removed on install and uninstall.
const OLD_SHORTCUTS: [&str; 2] = [
    "Zapret Desktop.lnk",
    "Zapret Desktop — остановить WinDivert.lnk",
];
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Iolit";
const OLD_UNINSTALL_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Zapret Desktop";
const NSIS_PRODUCT_KEY: &str = r"Software\zapret\Zapret Desktop";
const NSIS_MANUFACTURER_KEY: &str = r"Software\zapret";
/// Owner Administrators; SYSTEM and Administrators only; not inherited.
const DATA_SDDL: &str = "O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";
pub const WEBVIEW2_DOWNLOAD: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
const WEBVIEW2_CLIENT: &str =
    r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Install,
    Uninstall,
    StopDriver,
}

struct Options {
    mode: Mode,
    /// No window: for scripts, CI and the uninstall entry's quiet command.
    quiet: bool,
    /// Started by the service to update the app: quiet, puts the previous
    /// version back on failure, then opens the window in the user's session.
    update: bool,
}

/// What a successful install does next.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum After {
    Nothing,
    /// The window installer is run by the user: open the app as that user.
    OpenApp,
    /// Run by the service as SYSTEM: open the app in the signed-in session.
    Reopen,
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Options> {
    let mut mode = None;
    let mut quiet = false;
    let mut update = false;
    for arg in args {
        let next = match arg.as_str() {
            "--uninstall" => Mode::Uninstall,
            "--stop-windivert" => Mode::StopDriver,
            "--quiet" => {
                quiet = true;
                continue;
            }
            "--update" => {
                update = true;
                quiet = true;
                continue;
            }
            _ => bail!(
                "Неизвестный параметр {arg}. Можно: --uninstall, --stop-windivert, --quiet, --update"
            ),
        };
        if mode.replace(next).is_some() {
            bail!("Укажите только одно действие");
        }
    }
    let mode = mode.unwrap_or(Mode::Install);
    if update && mode != Mode::Install {
        bail!("--update только для установки");
    }
    Ok(Options {
        mode,
        quiet,
        update,
    })
}

pub fn main(payload: &'static [u8]) -> ExitCode {
    let options = match parse_args(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(e) => {
            crate::gui::message(&format!("{e:#}"));
            return ExitCode::from(2);
        }
    };
    if !options.quiet {
        return crate::gui::run(options.mode, payload);
    }
    attach_console();
    let after = if options.update {
        // Started by the service with no console: the steps go to a file
        // next to the downloaded installer.
        log_to_file();
        After::Reopen
    } else {
        After::Nothing
    };
    report(Event::Info(format!(
        "{PRODUCT} {VERSION}, время {}\n",
        unix_now()
    )));
    match run(options.mode, payload, after) {
        Ok(()) => {
            report(Event::Info("\nГотово".into()));
            ExitCode::SUCCESS
        }
        Err(e) => {
            report(Event::Info(format!("\nНе получилось: {e:#}")));
            ExitCode::FAILURE
        }
    }
}

/// The update log, in the folder of the running installer
/// (`<install>\\updates`, written only by administrators).
const UPDATE_LOG: &str = "update-log.txt";

fn log_to_file() {
    let Some(path) = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join(UPDATE_LOG)))
    else {
        return;
    };
    let Ok(file) = fs::File::create(path) else {
        return;
    };
    let file = std::sync::Mutex::new(file);
    set_reporter(Box::new(move |event| {
        use std::io::Write;
        if let Ok(mut file) = file.lock() {
            let _ = file.write_all(event_text(event).as_bytes());
            let _ = file.flush();
        }
    }));
}

/// The installer is a window app; a quiet run still prints into the console
/// that started it, so CI logs show the steps.
fn attach_console() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// Runs one action; `after` says what follows a successful install.
pub fn run(mode: Mode, payload: &[u8], after: After) -> Result<()> {
    match mode {
        Mode::Install => install(payload, after),
        Mode::Uninstall => uninstall(),
        Mode::StopDriver => stop_windivert(),
    }
}

/// Progress of an action, shown in the window or printed in quiet mode.
pub enum Event {
    /// Number of steps the action will run.
    Plan(usize),
    Step(String),
    StepDone {
        ok: bool,
        seconds: f32,
        notes: Vec<String>,
    },
    /// Something the user should read after the action.
    Note(String),
    /// Console-only summary line.
    Info(String),
}

type Reporter = Box<dyn Fn(Event) + Send + Sync>;
static REPORTER: OnceLock<Reporter> = OnceLock::new();

/// The window sets this once; without it events go to the console.
pub fn set_reporter(reporter: Reporter) {
    let _ = REPORTER.set(reporter);
}

fn report(event: Event) {
    if let Some(reporter) = REPORTER.get() {
        return reporter(event);
    }
    print!("{}", event_text(event));
}

/// An event as console or log text.
fn event_text(event: Event) -> String {
    match event {
        Event::Plan(_) => String::new(),
        Event::Step(name) => format!("{name}… "),
        Event::StepDone { ok, seconds, notes } => {
            let outcome = if ok { "готово" } else { "ошибка" };
            let mut text = format!("{outcome} ({seconds:.1} с)\n");
            for note in notes {
                text += &format!("    {note}\n");
            }
            text
        }
        Event::Note(text) | Event::Info(text) => format!("{text}\n"),
    }
}

/// Runs one named step and reports how long it took, so testers can tell
/// which step is slow. Notes from the step are shown under it.
fn step<T>(name: &str, run: impl FnOnce(&mut Vec<String>) -> Result<T>) -> Result<T> {
    report(Event::Step(name.to_owned()));
    let started = Instant::now();
    let mut notes = Vec::new();
    let result = run(&mut notes);
    report(Event::StepDone {
        ok: result.is_ok(),
        seconds: started.elapsed().as_secs_f32(),
        notes,
    });
    result.with_context(|| name.to_owned())
}

/// Where settings and the user's domains stay after uninstall.
pub fn data_dir() -> Option<PathBuf> {
    Paths::detect().ok().map(|paths| paths.data)
}

fn install(bytes: &[u8], after: After) -> Result<()> {
    report(Event::Plan(11));
    let _guard = step("Проверка системы", |_| {
        require_admin()?;
        reject_arm()?;
        single_instance()
    })?;
    let entries = step("Проверка файлов установщика", |_| payload::parse(bytes))?;
    let paths = Paths::detect()?;
    // A service up to 0.2.2 that started this update keeps its operation gate
    // until its own install timeout (180 s) runs out and only then stops.
    let stop_wait = if after == After::Reopen {
        UPDATE_STOP_WAIT
    } else {
        STOP_WAIT
    };
    let had_service = step("Остановка службы", |_| {
        stop_service(stop_wait)
    })?;
    // The service is stopped from here on: no failure may leave it so.
    let prepared = (|| {
        step("Закрытие окна программы", |notes| {
            close_app(&paths.app(), notes)
        })?;
        step("Выгрузка драйвера WinDivert", |notes| {
            release_driver(&paths.install, false, notes)
        })?;
        step("Копия прежней версии", |notes| {
            backup_files(&paths, notes)
        })
    })();
    let backup = match prepared {
        Ok(backup) => backup,
        Err(e) if had_service => return Err(restart_previous(&paths, after, e)),
        Err(e) => return Err(e),
    };
    let installed = (|| {
        let record = step("Копирование файлов", |notes| {
            copy_files(&paths, entries, notes)
        })?;
        step("Защита папки данных", |notes| {
            secure_data_dir(&paths.data, notes)
        })?;
        step("Регистрация службы", |_| {
            register_service(&paths)
        })?;
        step(
            "Ярлыки и запись в «Приложения»",
            |notes| {
                if let Err(e) = create_shortcuts(&paths) {
                    notes.push(format!("ярлыки не созданы: {e:#}"));
                }
                register_uninstall(&paths, &record)
            },
        )?;
        step("Запуск службы", |_| start_service())
    })();
    if let Err(e) = installed {
        if let Some(backup) = &backup {
            let restored = step("Возврат прежней версии", |notes| {
                restore_files(&paths, backup, notes)
            });
            if after == After::Reopen {
                let _ = reopen_app(&paths);
            }
            return Err(match restored {
                Ok(()) => e.context("прежняя версия возвращена"),
                Err(r) => e.context(format!("прежнюю версию вернуть не удалось: {r:#}")),
            });
        }
        // No copy of the earlier version (its file list was unreadable): the
        // service that was there gets started on whatever files it has.
        if had_service {
            return Err(restart_previous(&paths, after, e));
        }
        return Err(e);
    }
    if let Some(backup) = backup {
        let _ = fs::remove_dir_all(backup);
    }
    report(Event::Info(format!(
        "\n{PRODUCT} установлен в {}.\nОбход выключен, пока его не включат в программе.",
        paths.install.display()
    )));
    if !webview2_installed() {
        report(Event::Note(format!(
            "Не найден Microsoft Edge WebView2. Если окно программы не откроется, \
             установите его: {WEBVIEW2_DOWNLOAD}"
        )));
    }
    let opened = match after {
        After::Nothing => Ok(()),
        After::OpenApp => launch_as_user(&paths),
        After::Reopen => reopen_app(&paths),
    };
    if let Err(e) = opened {
        report(Event::Note(format!(
            "Не удалось открыть программу ({e:#}); она есть в меню «Пуск»."
        )));
    }
    Ok(())
}

/// After a failure with the files unchanged: starts the service that was
/// stopped and, for an update, opens the window again.
fn restart_previous(paths: &Paths, after: After, error: anyhow::Error) -> anyhow::Error {
    let started = step("Запуск службы", |_| start_service());
    if after == After::Reopen
        && let Err(e) = reopen_app(paths)
    {
        report(Event::Note(format!("Не удалось открыть программу ({e:#})")));
    }
    match started {
        Ok(()) => error.context("служба запущена снова"),
        Err(e) => error.context(format!("службу запустить не удалось: {e:#}")),
    }
}

/// Folder with the files of the version being replaced, inside the
/// admin-only install folder.
const BACKUP: &str = ".previous";

/// Copies the files the previous installer recorded, so a failed update can
/// put them back. `None` on a first install.
fn backup_files(paths: &Paths, notes: &mut Vec<String>) -> Result<Option<PathBuf>> {
    let Ok(bytes) = fs::read(paths.record()) else {
        return Ok(None);
    };
    let record = match Record::parse(&bytes) {
        Ok(record) => record,
        Err(e) => {
            notes.push(format!(
                "старый список файлов не прочитан, копии не будет: {e:#}"
            ));
            return Ok(None);
        }
    };
    let backup = paths.install.join(BACKUP);
    if backup.exists() {
        fs::remove_dir_all(&backup)?;
    }
    // Created here, not only for the first file: a record with no files
    // still needs the folder for `installed.json`.
    fs::create_dir_all(&backup)?;
    for file in &record.files {
        let source = payload::join(&paths.install, file);
        let target = payload::join(&backup, file);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Err(e) = fs::copy(&source, &target) {
            notes.push(format!("{file} не скопирован: {e}"));
        }
    }
    fs::write(backup.join("installed.json"), &bytes)?;
    notes.push(format!("версия {}", record.version));
    Ok(Some(backup))
}

/// Puts the backed-up version back and starts its service.
fn restore_files(paths: &Paths, backup: &Path, notes: &mut Vec<String>) -> Result<()> {
    let _ = stop_service(STOP_WAIT);
    let bytes = fs::read(backup.join("installed.json"))?;
    let old = Record::parse(&bytes)?;
    // Files only the new version added.
    if let Ok(new) = fs::read(paths.record())
        .map_err(anyhow::Error::from)
        .and_then(|b| Record::parse(&b))
    {
        for stale in new.stale(&old) {
            remove_or_schedule(&payload::join(&paths.install, stale), notes);
        }
    }
    for file in &old.files {
        if let Ok(data) = fs::read(payload::join(backup, file)) {
            write_replacing(&payload::join(&paths.install, file), &data, notes)?;
        }
    }
    fs::write(paths.record(), &bytes)?;
    register_service(paths)?;
    start_service()?;
    let _ = fs::remove_dir_all(backup);
    Ok(())
}

/// Opens the app in the session of the user at the screen. The updater runs
/// as SYSTEM, which has no desktop of its own.
fn reopen_app(paths: &Paths) -> Result<()> {
    use windows_sys::Win32::System::{
        Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock},
        RemoteDesktop::{WTSGetActiveConsoleSessionId, WTSQueryUserToken},
        Threading::{
            CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW, PROCESS_INFORMATION, STARTUPINFOW,
        },
    };
    let app = paths.app();
    let application = wide(&app);
    let mut command_line = wide(format!("\"{}\"", app.display()));
    let directory = wide(&paths.install);
    let mut desktop = wide("winsta0\\default");
    unsafe {
        let session = WTSGetActiveConsoleSessionId();
        if session == u32::MAX {
            bail!("никто не вошёл в систему");
        }
        let mut token: HANDLE = std::ptr::null_mut();
        if WTSQueryUserToken(session, &mut token) == 0 {
            return Err(io::Error::last_os_error()).context("нет пользователя у экрана");
        }
        let token = Handle(token);
        let mut environment = std::ptr::null_mut();
        let has_environment = CreateEnvironmentBlock(&mut environment, token.0, 0) != 0;
        let mut startup: STARTUPINFOW = std::mem::zeroed();
        startup.cb = size_of::<STARTUPINFOW>() as u32;
        startup.lpDesktop = desktop.as_mut_ptr();
        let mut process: PROCESS_INFORMATION = std::mem::zeroed();
        let ok = CreateProcessAsUserW(
            token.0,
            application.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            if has_environment {
                CREATE_UNICODE_ENVIRONMENT
            } else {
                0
            },
            if has_environment {
                environment
            } else {
                std::ptr::null()
            },
            directory.as_ptr(),
            &startup,
            &mut process,
        );
        let error = io::Error::last_os_error();
        if has_environment {
            DestroyEnvironmentBlock(environment);
        }
        if ok == 0 {
            return Err(error.into());
        }
        CloseHandle(process.hProcess);
        CloseHandle(process.hThread);
    }
    Ok(())
}

fn uninstall() -> Result<()> {
    report(Event::Plan(7));
    let _guard = step("Проверка прав", |_| {
        require_admin()?;
        single_instance()
    })?;
    let paths = Paths::detect()?;
    step("Возврат настроек DNS", |notes| {
        restore_dns(&paths, notes);
        Ok(())
    })?;
    step("Остановка и удаление службы", |_| remove_service())?;
    step("Закрытие окна программы", |notes| {
        close_app(&paths.app(), notes)
    })?;
    step("Выгрузка драйвера WinDivert", |notes| {
        release_driver(&paths.install, true, notes)
    })?;
    step(
        "Удаление ярлыков и записи в «Приложения»",
        |_| remove_registration(&paths),
    )?;
    step("Удаление файлов", |notes| {
        remove_tree(&paths.install, notes)
    })?;
    report(Event::Info(format!("\n{PRODUCT} удалён.")));
    Ok(())
}

/// The one-button fix when WinDivert is stuck or held: ends every winws (ours
/// or another zapret's) and stops and unregisters the driver whoever loaded
/// it, as Flowseal's service.bat does. WinDivert.dll registers it again on
/// next use; our service shows the engine as stopped until it is enabled.
fn stop_windivert() -> Result<()> {
    report(Event::Plan(2 + DRIVERS.len()));
    let _guard = step("Проверка прав", |_| {
        require_admin()?;
        single_instance()
    })?;
    step("Остановка winws", |notes| {
        notes.push(match end_processes("winws.exe", None)? {
            0 => "не был запущен".to_owned(),
            count => format!("остановлено процессов: {count}"),
        });
        Ok(())
    })?;
    for name in DRIVERS {
        step(
            &format!("Остановка драйвера {name}"),
            |notes| stop_driver(name, notes),
        )?;
    }
    report(Event::Info(
        "\nWinDivert остановлен. Обход выключен до перезапуска компьютера или до включения \
         в программе."
            .to_owned(),
    ));
    Ok(())
}

fn stop_driver(name: &str, notes: &mut Vec<String>) -> Result<()> {
    let manager = manager(ServiceManagerAccess::CONNECT)?;
    let access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;
    let Some(driver) = open_service(&manager, name, access)? else {
        notes.push("не загружен".into());
        return Ok(());
    };
    wait_stopped(&driver, Duration::from_secs(15)).context(
        "драйвер держит другая программа (например, GoodbyeDPI): закройте её и повторите",
    )?;
    delete_service(&driver)
}

struct Shortcut {
    link: PathBuf,
    target: PathBuf,
    arguments: &'static str,
    description: &'static str,
}

struct Paths {
    install: PathBuf,
    data: PathBuf,
    programs: PathBuf,
    desktop: PathBuf,
    windows: PathBuf,
}

impl Paths {
    /// Known folders come from the registry, not from environment variables
    /// the launching user could change.
    fn detect() -> Result<Self> {
        use ::windows::Win32::UI::Shell::{
            FOLDERID_CommonPrograms, FOLDERID_ProgramData, FOLDERID_ProgramFiles,
            FOLDERID_PublicDesktop, FOLDERID_Windows,
        };
        let install = payload::install_dir(&known_folder(&FOLDERID_ProgramFiles)?);
        Ok(Self {
            data: payload::data_dir(&known_folder(&FOLDERID_ProgramData)?, &install),
            install,
            programs: known_folder(&FOLDERID_CommonPrograms)?,
            desktop: known_folder(&FOLDERID_PublicDesktop)?,
            windows: known_folder(&FOLDERID_Windows)?,
        })
    }
    fn app(&self) -> PathBuf {
        self.install.join(APP_EXE)
    }
    fn service(&self) -> PathBuf {
        self.install.join("runtime").join("zapretd.exe")
    }
    fn record(&self) -> PathBuf {
        self.install.join(payload::RECORD)
    }
    fn uninstaller(&self) -> PathBuf {
        self.install.join(UNINSTALLER)
    }
    /// Shortcuts the Zapret Desktop versions made, in both places.
    fn old_shortcuts(&self) -> Vec<PathBuf> {
        OLD_SHORTCUTS
            .iter()
            .flat_map(|name| [self.programs.join(name), self.desktop.join(name)])
            .collect()
    }
    fn shortcuts(&self) -> [Shortcut; 3] {
        let app = |link| Shortcut {
            link,
            target: self.app(),
            arguments: "",
            description: "Управление локальным обходом DPI",
        };
        [
            app(self.programs.join(SHORTCUT)),
            app(self.desktop.join(SHORTCUT)),
            Shortcut {
                link: self.programs.join(STOP_SHORTCUT),
                target: self.uninstaller(),
                arguments: "--stop-windivert",
                description: "Остановить WinDivert и winws одной кнопкой",
            },
        ]
    }
}

fn known_folder(id: &::windows::core::GUID) -> Result<PathBuf> {
    use ::windows::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{KF_FLAG_DEFAULT, SHGetKnownFolderPath},
    };
    unsafe {
        let raw = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None)?;
        let path = raw.to_string();
        CoTaskMemFree(Some(raw.as_ptr() as *const _));
        Ok(PathBuf::from(path?))
    }
}

fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain(Some(0)).collect()
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn require_admin() -> Result<()> {
    let elevated = unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            false
        } else {
            let token = Handle(token);
            let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
            let mut size = 0;
            GetTokenInformation(
                token.0,
                TokenElevation,
                &mut elevation as *mut _ as *mut _,
                size_of::<TOKEN_ELEVATION>() as u32,
                &mut size,
            ) != 0
                && elevation.TokenIsElevated != 0
        }
    };
    if !elevated {
        bail!(
            "нужны права администратора: запустите установщик снова и подтвердите запрос Windows"
        );
    }
    Ok(())
}

/// The registry value is not emulated, unlike the process view under x64 emulation.
fn reject_arm() -> Result<()> {
    let arch: String = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment")
        .and_then(|key| key.get_value("PROCESSOR_ARCHITECTURE"))
        .unwrap_or_default();
    if arch.eq_ignore_ascii_case("ARM64") {
        bail!("Windows на ARM не поддерживается: драйвер WinDivert работает только на x64");
    }
    Ok(())
}

fn single_instance() -> Result<Handle> {
    let name = wide(r"Global\ZapretDesktopSetup");
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(io::Error::last_os_error().into());
    }
    let handle = Handle(handle);
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        bail!("установщик {PRODUCT} уже запущен");
    }
    Ok(handle)
}

fn os_error(error: &windows_service::Error) -> Option<u32> {
    match error {
        windows_service::Error::Winapi(e) => e.raw_os_error().map(|code| code as u32),
        _ => None,
    }
}

fn manager(access: ServiceManagerAccess) -> Result<ServiceManager> {
    Ok(ServiceManager::local_computer(None::<&str>, access)?)
}

fn open_service(
    manager: &ServiceManager,
    name: &str,
    access: ServiceAccess,
) -> Result<Option<Service>> {
    match manager.open_service(name, access) {
        Ok(service) => Ok(Some(service)),
        Err(e) if os_error(&e) == Some(ERROR_SERVICE_DOES_NOT_EXIST) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn wait_stopped(service: &Service, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        let state = service.query_status()?.current_state;
        if state == ServiceState::Stopped {
            return Ok(());
        }
        if state != ServiceState::StopPending
            && let Err(e) = service.stop()
            // A starting service accepts Stop only once it runs.
            && !matches!(
                os_error(&e),
                Some(ERROR_SERVICE_CANNOT_ACCEPT_CTRL | ERROR_SERVICE_NOT_ACTIVE)
            )
        {
            return Err(e.into());
        }
        if Instant::now() >= deadline {
            bail!("не остановилась за {} с", timeout.as_secs());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn delete_service(service: &Service) -> Result<()> {
    match service.delete() {
        Err(e) if os_error(&e) != Some(ERROR_SERVICE_MARKED_FOR_DELETE) => Err(e.into()),
        _ => Ok(()),
    }
}

const STOP_WAIT: Duration = Duration::from_secs(30);
/// Longer than `zapretd::app_update::INSTALL_TIMEOUT` (180 s), see `install`.
const UPDATE_STOP_WAIT: Duration = Duration::from_secs(240);

/// `true` when the service was installed.
fn stop_service(wait: Duration) -> Result<bool> {
    let manager = manager(ServiceManagerAccess::CONNECT)?;
    let access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP;
    let Some(service) = open_service(&manager, SERVICE, access)? else {
        return Ok(false);
    };
    wait_stopped(&service, wait)?;
    Ok(true)
}

fn remove_service() -> Result<()> {
    let manager = manager(ServiceManagerAccess::CONNECT)?;
    let access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;
    if let Some(service) = open_service(&manager, SERVICE, access)? {
        wait_stopped(&service, Duration::from_secs(30))?;
        delete_service(&service)?;
    }
    Ok(())
}

/// The WinDivert driver stays loaded after winws exits and keeps
/// WinDivert64.sys busy, so it is stopped (and deleted on uninstall) before
/// files change. Only a driver loaded from our folder is touched: another
/// program's WinDivert stays as it is.
fn release_driver(install: &Path, delete: bool, notes: &mut Vec<String>) -> Result<()> {
    let manager = manager(ServiceManagerAccess::CONNECT)?;
    let mut access =
        ServiceAccess::QUERY_CONFIG | ServiceAccess::QUERY_STATUS | ServiceAccess::STOP;
    if delete {
        access |= ServiceAccess::DELETE;
    }
    let Some(driver) = open_service(&manager, DRIVER, access)? else {
        return Ok(());
    };
    let image = driver.query_config()?.executable_path;
    if !payload::image_inside(&image.to_string_lossy(), install) {
        notes.push("WinDivert установлен другой программой, его не трогаю".into());
        return Ok(());
    }
    if let Err(e) = wait_stopped(&driver, Duration::from_secs(15)) {
        notes.push(format!(
            "драйвер не выгрузился ({e:#}); занятые файлы заменятся после перезагрузки"
        ));
    }
    if delete {
        delete_service(&driver)?;
    }
    Ok(())
}

/// Ends running copies of the installed app: its window keeps the exe busy.
fn close_app(app: &Path, notes: &mut Vec<String>) -> Result<()> {
    let closed = end_processes(APP_EXE, Some(app))?;
    if closed > 0 {
        notes.push(format!("закрыто окон программы: {closed}"));
    }
    Ok(())
}

/// Ends processes named `exe`; with `only`, just those started from that file.
fn end_processes(exe: &str, only: Option<&Path>) -> Result<usize> {
    let only = only.map(|path| path.to_string_lossy().to_lowercase());
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error().into());
    }
    let snapshot = Handle(snapshot);
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut ended = 0;
    let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
    while more {
        let len = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(0);
        let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
        if name.eq_ignore_ascii_case(exe) {
            let access =
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE;
            let process = unsafe { OpenProcess(access, 0, entry.th32ProcessID) };
            if !process.is_null() {
                let process = Handle(process);
                let matches = only.as_ref().is_none_or(|only| {
                    process_path(&process).is_some_and(|p| p.to_lowercase() == *only)
                });
                if matches {
                    unsafe {
                        TerminateProcess(process.0, 0);
                        WaitForSingleObject(process.0, 5000);
                    }
                    ended += 1;
                }
            }
        }
        more = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
    }
    Ok(ended)
}

fn process_path(process: &Handle) -> Option<String> {
    let mut buffer = [0u16; 1024];
    let mut len = buffer.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(process.0, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut len)
    };
    (ok != 0).then(|| String::from_utf16_lossy(&buffer[..len as usize]))
}

fn move_file(from: &Path, to: &Path, flags: u32) -> io::Result<()> {
    let (from, to) = (wide(from), wide(to));
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), flags) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn delete_at_reboot(path: &Path) -> io::Result<()> {
    let path = wide(path);
    if unsafe { MoveFileExW(path.as_ptr(), std::ptr::null(), MOVEFILE_DELAY_UNTIL_REBOOT) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// `name.ext` → `name.ext.zapret-<suffix>` in the same folder.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".zapret-{suffix}"));
    path.with_file_name(name)
}

fn unique() -> String {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Writes `data` to `target`. Windows will not overwrite a running program
/// or a loaded driver but lets it be renamed, so a busy file is moved aside
/// and deleted at the next reboot.
fn write_replacing(target: &Path, data: &[u8], notes: &mut Vec<String>) -> Result<()> {
    let fresh = sibling(target, "new");
    fs::write(&fresh, data)?;
    let flags = MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH;
    if move_file(&fresh, target, flags).is_ok() {
        return Ok(());
    }
    let aside = sibling(target, &format!("old-{}", unique()));
    let replaced = move_file(target, &aside, 0)
        .context("файл занят другой программой")
        .and_then(|()| move_file(&fresh, target, flags).map_err(Into::into));
    if let Err(e) = replaced {
        let _ = fs::remove_file(&fresh);
        return Err(e);
    }
    if delete_at_reboot(&aside).is_err() {
        notes.push(format!("удалите вручную: {}", aside.display()));
    }
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    notes.push(format!(
        "{name} был занят: заменён, старая копия удалится после перезагрузки"
    ));
    Ok(())
}

fn remove_or_schedule(path: &Path, notes: &mut Vec<String>) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(_) => {
            let aside = sibling(path, &format!("delete-{}", unique()));
            let target = if move_file(path, &aside, 0).is_ok() {
                aside
            } else {
                path.to_path_buf()
            };
            if delete_at_reboot(&target).is_err() {
                notes.push(format!("удалите вручную: {}", target.display()));
            }
        }
    }
}

fn copy_files(paths: &Paths, mut entries: Vec<Entry>, notes: &mut Vec<String>) -> Result<Record> {
    let previous = match fs::read(paths.record()) {
        Ok(bytes) => match Record::parse(&bytes) {
            Ok(record) => Some(record),
            Err(e) => {
                notes.push(format!("старый список файлов не прочитан: {e:#}"));
                None
            }
        },
        Err(_) => None,
    };
    // The uninstaller is this same program, run with --uninstall.
    let exe = std::env::current_exe()?;
    entries.push(Entry {
        path: UNINSTALLER.into(),
        data: fs::read(&exe).with_context(|| exe.display().to_string())?,
    });
    fs::create_dir_all(&paths.install)?;
    for entry in &entries {
        let target = payload::join(&paths.install, &entry.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        write_replacing(&target, &entry.data, notes)
            .with_context(|| format!("не удалось записать {}", entry.path))?;
    }
    let record = Record::new(VERSION, entries.iter().map(|e| e.path.clone()));
    if let Some(previous) = previous {
        for stale in previous.stale(&record) {
            remove_or_schedule(&payload::join(&paths.install, stale), notes);
        }
    }
    fs::write(paths.record(), serde_json::to_vec_pretty(&record)?)?;
    Ok(record)
}

/// A security descriptor parsed from SDDL, freed with LocalFree.
struct Descriptor(PSECURITY_DESCRIPTOR);

impl Descriptor {
    fn parse(sddl: &str) -> Result<Self> {
        let text = wide(sddl);
        let mut descriptor = std::ptr::null_mut();
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(Self(descriptor))
    }
    fn create_dir(&self, path: &Path) -> Result<()> {
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        };
        let path = wide(path);
        if unsafe { CreateDirectoryW(path.as_ptr(), &attributes) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok(())
    }
    /// Owner and protected DACL of an existing folder; inheritable entries
    /// reach the files inside.
    fn apply(&self, path: &Path) -> Result<()> {
        let mut owner = std::ptr::null_mut();
        let mut dacl = std::ptr::null_mut();
        let (mut present, mut defaulted) = (0, 0);
        unsafe {
            if GetSecurityDescriptorOwner(self.0, &mut owner, &mut defaulted) == 0
                || GetSecurityDescriptorDacl(self.0, &mut present, &mut dacl, &mut defaulted) == 0
            {
                return Err(io::Error::last_os_error().into());
            }
            let path = wide(path);
            let status = SetNamedSecurityInfoW(
                path.as_ptr(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION
                    | DACL_SECURITY_INFORMATION
                    | PROTECTED_DACL_SECURITY_INFORMATION,
                owner,
                std::ptr::null_mut(),
                dacl,
                std::ptr::null(),
            );
            if status != 0 {
                return Err(io::Error::from_raw_os_error(status as i32).into());
            }
        }
        Ok(())
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

/// The data folder in `%ProgramData%` holds what the SYSTEM service reads: the
/// enabled flag, the strategy and user domains. Standard users may create
/// folders in ProgramData, so the folder gets owner Administrators and a
/// protected ACL. A link or file planted in its place is moved away, never
/// followed; so is a folder whose ACL cannot be reset.
fn secure_data_dir(data: &Path, notes: &mut Vec<String>) -> Result<()> {
    let descriptor = Descriptor::parse(DATA_SDDL)?;
    let mut move_aside = |why: &str| -> Result<()> {
        let name = data.file_name().unwrap_or_default().to_string_lossy();
        let aside = data.with_file_name(format!("{name}.untrusted-{}", unix_now()));
        move_file(data, &aside, 0).with_context(|| format!("{why}; убрать её не удалось"))?;
        notes.push(format!("{why}; перенесено в {}", aside.display()));
        descriptor.create_dir(data)
    };
    match fs::symlink_metadata(data) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => descriptor.create_dir(data),
        Err(e) => Err(e.into()),
        Ok(meta)
            if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
                || meta.file_attributes() & FILE_ATTRIBUTE_DIRECTORY == 0 =>
        {
            move_aside("на месте папки данных была ссылка или файл")
        }
        Ok(_) => match descriptor.apply(data) {
            Ok(()) => Ok(()),
            Err(e) => move_aside(&format!("права папки данных не изменились ({e:#})")),
        },
    }
}

fn register_service(paths: &Paths) -> Result<()> {
    let manager = manager(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;
    let info = ServiceInfo {
        name: SERVICE.into(),
        display_name: SERVICE_DISPLAY.into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: paths.service(),
        launch_arguments: vec![],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };
    let marked = |e: windows_service::Error| {
        if os_error(&e) == Some(ERROR_SERVICE_MARKED_FOR_DELETE) {
            anyhow!(
                "старая служба ещё удаляется: перезагрузите компьютер и запустите установщик снова"
            )
        } else {
            e.into()
        }
    };
    let access = ServiceAccess::QUERY_STATUS | ServiceAccess::START | ServiceAccess::CHANGE_CONFIG;
    let service = match open_service(&manager, SERVICE, access)? {
        Some(service) => {
            service.change_config(&info).map_err(marked)?;
            service
        }
        None => manager.create_service(&info, access).map_err(marked)?,
    };
    service.set_description(SERVICE_DESCRIPTION)?;
    service.update_failure_actions(ServiceFailureActions {
        reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(86400)),
        reboot_msg: None,
        command: None,
        actions: Some(vec![
            ServiceAction {
                action_type: ServiceActionType::Restart,
                delay: Duration::from_secs(5),
            },
            ServiceAction {
                action_type: ServiceActionType::Restart,
                delay: Duration::from_secs(15),
            },
            ServiceAction {
                action_type: ServiceActionType::None,
                delay: Duration::ZERO,
            },
        ]),
    })?;
    Ok(())
}

fn start_service() -> Result<()> {
    let manager = manager(ServiceManagerAccess::CONNECT)?;
    let service =
        manager.open_service(SERVICE, ServiceAccess::QUERY_STATUS | ServiceAccess::START)?;
    if let Err(e) = service.start::<&OsStr>(&[])
        && os_error(&e) != Some(ERROR_SERVICE_ALREADY_RUNNING)
    {
        return Err(e.into());
    }
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let status = service.query_status()?;
        match status.current_state {
            ServiceState::Running => return Ok(()),
            ServiceState::Stopped => {
                bail!(
                    "служба остановилась сразу после запуска ({:?})",
                    status.exit_code
                )
            }
            _ if Instant::now() >= deadline => bail!("служба не запустилась за 90 с"),
            _ => std::thread::sleep(Duration::from_millis(250)),
        }
    }
}

fn create_shortcuts(paths: &Paths) -> Result<()> {
    use ::windows::{
        Win32::{
            System::Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize, IPersistFile,
            },
            UI::Shell::{IShellLinkW, ShellLink},
        },
        core::{HSTRING, Interface},
    };
    // Shortcuts under the old name would stay next to the new ones.
    for link in paths.old_shortcuts() {
        let _ = fs::remove_file(link);
    }
    let icon = HSTRING::from(paths.app().as_os_str());
    let folder = HSTRING::from(paths.install.as_os_str());
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = (|| -> ::windows::core::Result<()> {
            for shortcut in paths.shortcuts() {
                let shell_link: IShellLinkW =
                    CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
                shell_link.SetPath(&HSTRING::from(shortcut.target.as_os_str()))?;
                shell_link.SetArguments(&HSTRING::from(shortcut.arguments))?;
                shell_link.SetWorkingDirectory(&folder)?;
                shell_link.SetIconLocation(&icon, 0)?;
                shell_link.SetDescription(&HSTRING::from(shortcut.description))?;
                shell_link
                    .cast::<IPersistFile>()?
                    .Save(&HSTRING::from(shortcut.link.as_os_str()), true)?;
            }
            Ok(())
        })();
        if initialized {
            CoUninitialize();
        }
        result?;
    }
    Ok(())
}

fn quoted(path: &Path, arguments: &str) -> OsString {
    let mut text = OsString::from("\"");
    text.push(path);
    text.push("\"");
    text.push(arguments);
    text
}

fn register_uninstall(paths: &Paths, record: &Record) -> Result<()> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    // The entry of the Zapret Desktop versions (and of the NSIS installer
    // before them) goes, so «Приложения» lists the program once.
    for key in [UNINSTALL_KEY, OLD_UNINSTALL_KEY] {
        if let Err(e) = hklm.delete_subkey_all(key)
            && e.kind() != io::ErrorKind::NotFound
        {
            return Err(e.into());
        }
    }
    let (key, _) = hklm.create_subkey(UNINSTALL_KEY)?;
    let size: u64 = record
        .files
        .iter()
        .filter_map(|f| fs::metadata(payload::join(&paths.install, f)).ok())
        .map(|m| m.len())
        .sum();
    key.set_value("DisplayName", &PRODUCT)?;
    key.set_value("DisplayVersion", &VERSION)?;
    key.set_value("Publisher", &PUBLISHER)?;
    key.set_value("DisplayIcon", &paths.app().into_os_string())?;
    key.set_value("InstallLocation", &paths.install.clone().into_os_string())?;
    key.set_value(
        "UninstallString",
        &quoted(&paths.uninstaller(), " --uninstall"),
    )?;
    key.set_value(
        "QuietUninstallString",
        &quoted(&paths.uninstaller(), " --uninstall --quiet"),
    )?;
    key.set_value("NoModify", &1u32)?;
    key.set_value("NoRepair", &1u32)?;
    key.set_value(
        "EstimatedSize",
        &u32::try_from(size / 1024).unwrap_or(u32::MAX),
    )?;
    remove_nsis_keys(&hklm);
    Ok(())
}

/// The NSIS installer of earlier builds kept its folder here.
fn remove_nsis_keys(hklm: &RegKey) {
    let _ = hklm.delete_subkey_all(NSIS_PRODUCT_KEY);
    // Fails while other programs of this publisher keep subkeys there.
    let _ = hklm.delete_subkey(NSIS_MANUFACTURER_KEY);
}

fn remove_registration(paths: &Paths) -> Result<()> {
    let links = paths
        .shortcuts()
        .into_iter()
        .map(|s| s.link)
        .chain(paths.old_shortcuts());
    for link in links {
        if let Err(e) = fs::remove_file(&link)
            && e.kind() != io::ErrorKind::NotFound
        {
            return Err(anyhow!(e).context(link.display().to_string()));
        }
    }
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for key in [UNINSTALL_KEY, OLD_UNINSTALL_KEY] {
        if let Err(e) = hklm.delete_subkey_all(key)
            && e.kind() != io::ErrorKind::NotFound
        {
            return Err(e.into());
        }
    }
    remove_nsis_keys(&hklm);
    Ok(())
}

/// Deletes the install folder without following links. Busy files (this
/// uninstaller, a driver that did not unload) are renamed and deleted at
/// the next reboot, then the folder itself.
fn remove_tree(dir: &Path, notes: &mut Vec<String>) -> Result<()> {
    let mut pending = 0;
    remove_inside(dir, &mut pending)?;
    match fs::remove_dir(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(_) => {
            delete_at_reboot(dir)?;
            pending += 1;
        }
    }
    if pending > 0 {
        notes.push(format!(
            "занятых файлов и папок: {pending}; они удалятся после перезагрузки"
        ));
    }
    Ok(())
}

fn remove_inside(dir: &Path, pending: &mut usize) -> Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let path = entry?.path();
        let attributes = fs::symlink_metadata(&path)?.file_attributes();
        let directory = attributes & FILE_ATTRIBUTE_DIRECTORY != 0;
        let link = attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0;
        if directory && link {
            // Remove the link itself, never what it points to.
            fs::remove_dir(&path)?;
        } else if directory {
            remove_inside(&path, pending)?;
            if fs::remove_dir(&path).is_err() {
                delete_at_reboot(&path)?;
                *pending += 1;
            }
        } else if fs::remove_file(&path).is_err() {
            let aside = sibling(&path, &format!("delete-{}", unique()));
            let target = if move_file(&path, &aside, 0).is_ok() {
                aside
            } else {
                path
            };
            delete_at_reboot(&target)?;
            *pending += 1;
        }
    }
    Ok(())
}

pub fn webview2_installed() -> bool {
    let present = |root: RegKey, path: &str| {
        root.open_subkey(path)
            .and_then(|key| key.get_value::<String, _>("pv"))
            .is_ok_and(|version| !version.is_empty() && version != "0.0.0.0")
    };
    present(
        RegKey::predef(HKEY_LOCAL_MACHINE),
        &format!(r"SOFTWARE\WOW6432Node\{WEBVIEW2_CLIENT}"),
    ) || present(
        RegKey::predef(HKEY_CURRENT_USER),
        &format!(r"Software\{WEBVIEW2_CLIENT}"),
    )
}

/// Opens the app as the signed-in user, not elevated: explorer passes the
/// request to the user's desktop shell.
fn launch_as_user(paths: &Paths) -> Result<()> {
    Command::new(paths.windows.join("explorer.exe"))
        .arg(paths.app())
        .spawn()?;
    Ok(())
}

/// Secure DNS turned on in the app must not outlive it: the service binary
/// restores each adapter's saved settings. A failure only leaves a note; the
/// user can still change DNS in Windows settings.
fn restore_dns(paths: &Paths, notes: &mut Vec<String>) {
    let service = paths.service();
    if !service.exists() {
        return;
    }
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    match Command::new(&service)
        .arg("--restore-dns")
        .creation_flags(CREATE_NO_WINDOW)
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => notes.push(format!(
            "настройки DNS не вернулись (код {}); проверьте их в «Параметрах» → «Сеть и Интернет»",
            status.code().unwrap_or(-1)
        )),
        Err(e) => notes.push(format!("настройки DNS не вернулись: {e}")),
    }
}

/// Opens the WebView2 download page in the user's browser, not elevated.
pub fn open_webview2_download() -> Result<()> {
    let windows = known_folder(&::windows::Win32::UI::Shell::FOLDERID_Windows)?;
    Command::new(windows.join("explorer.exe"))
        .arg(WEBVIEW2_DOWNLOAD)
        .spawn()?;
    Ok(())
}
