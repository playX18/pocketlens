use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

pub fn setup_camera(device: &str) -> Result<()> {
    let video_nr = device
        .strip_prefix("/dev/video")
        .context("device path must start with /dev/video")?;

    if !Path::new("/sys/module/v4l2loopback").exists() {
        Command::new("pkexec")
            .args(["modprobe", "-r", "v4l2loopback"])
            .status()
            .ok();
    }

    let status = Command::new("pkexec")
        .args([
            "modprobe",
            "v4l2loopback",
            &format!("video_nr={video_nr}"),
            "card_label=ACamera",
            "exclusive_caps=1",
        ])
        .status()
        .context("failed to run pkexec modprobe")?;

    if !status.success() {
        anyhow::bail!("modprobe v4l2loopback failed with {status}");
    }
    Ok(())
}

pub fn remove_camera() -> Result<()> {
    let status = Command::new("pkexec")
        .args(["modprobe", "-r", "v4l2loopback"])
        .status()
        .context("failed to run pkexec modprobe -r")?;

    if !status.success() {
        anyhow::bail!("modprobe -r v4l2loopback failed with {status}");
    }
    Ok(())
}

pub fn camera_ready() -> bool {
    Path::new("/sys/module/v4l2loopback").exists()
}
