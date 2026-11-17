//! Embeds the payload made by `zbuild installer-payload` (path in
//! ZAPRET_SETUP_PAYLOAD), the icon, and a manifest that asks Windows for
//! administrator rights at launch.
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=ZAPRET_SETUP_PAYLOAD");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("payload.tar.gz");
    match env::var_os("ZAPRET_SETUP_PAYLOAD") {
        Some(path) => {
            println!("cargo:rerun-if-changed={}", PathBuf::from(&path).display());
            fs::copy(&path, &out).expect("ZAPRET_SETUP_PAYLOAD must point to the payload");
        }
        // Workspace checks and tests build without a payload; such an
        // installer refuses to run.
        None => fs::write(&out, b"").expect("empty payload"),
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let icon = "../../src-tauri/icons/icon.ico";
        println!("cargo:rerun-if-changed={icon}");
        let mut res = tauri_winres::WindowsResource::new();
        res.set_icon(icon)
            .set("FileDescription", "Установка Iolit")
            .set("ProductName", "Iolit")
            .set_manifest(MANIFEST);
        res.compile().expect("Windows resources");
    }
}

/// Program Files, services and HKLM need elevation: UAC asks once at start.
/// Common Controls 6 and per-monitor DPI keep message boxes and the window sharp.
const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0"
        processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
</assembly>
"#;
