use crate::diagnostics::{DependencyReport, SystemProbe};

pub fn check_deps() -> String {
    let report = DependencyReport::from_probe(&SystemProbe);
    let mut lines = Vec::new();

    for dep in &report.dependencies {
        let icon = if dep.present {
            "\x1b[32m●\x1b[0m"
        } else {
            "\x1b[31m●\x1b[0m"
        };
        lines.push(format!("  {icon} {} — {}", dep.name, dep.detail));
    }

    let camera_icon = if report.virtual_camera_ready {
        "\x1b[32m●\x1b[0m"
    } else {
        "\x1b[31m●\x1b[0m"
    };
    let mic_icon = if report.virtual_microphone_ready {
        "\x1b[32m●\x1b[0m"
    } else {
        "\x1b[31m●\x1b[0m"
    };

    let summary = format!(
        "\n{camera_icon} Virtual camera: {}\n{mic_icon} Virtual microphone: {}\n",
        if report.virtual_camera_ready {
            "ready"
        } else {
            "not ready"
        },
        if report.virtual_microphone_ready {
            "ready"
        } else {
            "not ready"
        },
    );

    let mut result = lines.join("\n");
    result.push_str(&summary);

    result
}
