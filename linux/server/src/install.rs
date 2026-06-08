use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

pub fn install_app(prefix: &Path) -> Result<()> {
    let bin_dir = prefix.join("bin");
    let app_dir = prefix.join("share/applications");
    let data_dir = prefix.join("share/acamera");

    fs::create_dir_all(&bin_dir).context("creating bin directory")?;
    fs::create_dir_all(&app_dir).context("creating applications directory")?;
    fs::create_dir_all(&data_dir).context("creating data directory")?;

    let self_path = std::env::current_exe().context("resolving own path")?;
    let self_dir = self_path
        .parent()
        .context("resolving own parent directory")?;

    for bin_name in &["acamera-receiver", "acamera-gtk"] {
        let src = self_dir.join(bin_name);
        if src.exists() {
            let dest = bin_dir.join(bin_name);
            fs::copy(&src, &dest).context(format!("copying {bin_name}"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&dest, fs::Permissions::from_mode(0o755))?;
            }
        }
    }

    let apk_src = self_dir
        .parent()
        .map(|p| p.join("share/acamera/acamera.apk"))
        .filter(|p| p.exists());
    if let Some(apk) = apk_src {
        fs::copy(&apk, data_dir.join("acamera.apk")).context("copying APK")?;
    }

    let exec_path = bin_dir.join("acamera-gtk");
    let desktop = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=ACamera\n\
         Comment=Use an Android phone as a Linux camera and microphone\n\
         Exec={}\n\
         Terminal=false\n\
         Categories=AudioVideo;Video;\n\
         Icon=camera-web\n",
        exec_path.display()
    );
    fs::write(app_dir.join("acamera.desktop"), desktop).context("writing .desktop file")?;

    println!("installed ACamera to {}", prefix.display());
    println!("run: {}", exec_path.display());
    Ok(())
}

pub fn bundled_apk_path() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidates = [
            dir.join("../share/acamera/acamera.apk"),
            dir.join("share/acamera/acamera.apk"),
        ];
        for candidate in &candidates {
            if candidate.exists() {
                return Some(candidate.clone());
            }
        }
    }
    None
}

pub fn check_adb() -> bool {
    Command::new("sh")
        .arg("-c")
        .arg("command -v adb >/dev/null 2>&1")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
pub struct AdbDevice {
    pub serial: String,
    pub state: String,
    pub model: Option<String>,
}

pub fn list_adb_devices() -> Result<Vec<AdbDevice>> {
    let output = Command::new("adb")
        .args(["devices", "-l"])
        .output()
        .context("failed to run adb devices")?;

    if !output.status.success() {
        anyhow::bail!(
            "adb devices failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8(output.stdout)?;
    let mut devices = Vec::new();

    for line in stdout.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() || line == "List of devices attached" {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let serial = parts[0].to_string();
            let state = parts[1].to_string();
            let model = parts
                .iter()
                .find(|p| p.starts_with("model:"))
                .map(|p| p.trim_start_matches("model:").to_string());
            devices.push(AdbDevice {
                serial,
                state,
                model,
            });
        }
    }

    Ok(devices)
}

pub fn install_apk(device_serial: &str) -> Result<String> {
    let apk = bundled_apk_path().context("bundled APK not found")?;

    let output = Command::new("adb")
        .args(["-s", device_serial, "install", "-r"])
        .arg(&apk)
        .output()
        .context("failed to run adb install")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        anyhow::bail!("adb install failed: {stderr}");
    }

    Ok(format!("{stdout}\n{stderr}"))
}
