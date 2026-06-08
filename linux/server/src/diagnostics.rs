use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::{
    protocol::{DependencyStatus, Diagnostic, DiagnosticCode, DiagnosticSeverity},
    virtual_mic,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyReport {
    pub dependencies: Vec<DependencyStatus>,
    pub virtual_camera_ready: bool,
    pub virtual_microphone_ready: bool,
}

impl DependencyReport {
    pub fn from_probe<P: DependencyProbe>(probe: &P) -> Self {
        let dependencies = vec![
            probe.check("v4l2loopback", ProbeKind::KernelModule),
            probe.check("pipewire", ProbeKind::Command),
            probe.check("pactl", ProbeKind::Command),
            probe.check("gst-launch-1.0", ProbeKind::Command),
            probe.check(virtual_mic::DEFAULT_SOURCE_NAME, ProbeKind::PulseSource),
        ];
        let virtual_camera_ready = dependencies
            .iter()
            .any(|dep| dep.name == "v4l2loopback" && dep.present);
        let virtual_microphone_ready = dependencies
            .iter()
            .any(|dep| dep.name == "pipewire" && dep.present)
            && dependencies
                .iter()
                .any(|dep| dep.name == "pactl" && dep.present)
            && dependencies
                .iter()
                .any(|dep| dep.name == "gst-launch-1.0" && dep.present);
        let virtual_microphone_ready = virtual_microphone_ready
            && dependencies
                .iter()
                .any(|dep| dep.name == virtual_mic::DEFAULT_SOURCE_NAME && dep.present);
        Self {
            dependencies,
            virtual_camera_ready,
            virtual_microphone_ready,
        }
    }

    pub fn probe_system() -> Self {
        Self::from_probe(&SystemProbe)
    }

    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        if !self.dependency_present("v4l2loopback") {
            diagnostics.push(Diagnostic {
                code: DiagnosticCode::MissingV4l2loopback,
                severity: DiagnosticSeverity::Error,
                message:
                    "v4l2loopback is not loaded; create a virtual camera before starting a session."
                        .to_string(),
            });
        }
        if !self.dependency_present("pipewire") {
            diagnostics.push(Diagnostic {
                code: DiagnosticCode::MissingPipewire,
                severity: DiagnosticSeverity::Error,
                message: "PipeWire is not running; start PipeWire before exposing the virtual microphone."
                    .to_string(),
            });
        }
        if !self.dependency_present("gst-launch-1.0") {
            diagnostics.push(Diagnostic {
                code: DiagnosticCode::MissingGstreamer,
                severity: DiagnosticSeverity::Error,
                message: "GStreamer RTP decode plugins are not available.".to_string(),
            });
        }
        diagnostics
    }

    fn dependency_present(&self, name: &str) -> bool {
        self.dependencies
            .iter()
            .any(|dependency| dependency.name == name && dependency.present)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ProbeKind {
    Command,
    KernelModule,
    PulseSource,
}

pub trait DependencyProbe {
    fn check(&self, name: &str, kind: ProbeKind) -> DependencyStatus;
}

pub struct SystemProbe;

impl DependencyProbe for SystemProbe {
    fn check(&self, name: &str, kind: ProbeKind) -> DependencyStatus {
        match kind {
            ProbeKind::Command => {
                let present = Command::new("sh")
                    .arg("-c")
                    .arg(format!("command -v {name} >/dev/null 2>&1"))
                    .status()
                    .map(|status| status.success())
                    .unwrap_or(false);
                DependencyStatus {
                    name: name.to_string(),
                    present,
                    detail: if present {
                        "found on PATH".to_string()
                    } else {
                        format!("{name} was not found on PATH")
                    },
                }
            }
            ProbeKind::PulseSource => {
                let present = Command::new("pactl")
                    .args(["list", "short", "sources"])
                    .output()
                    .map(|output| {
                        output.status.success()
                            && String::from_utf8(output.stdout)
                                .map(|stdout| virtual_mic::parse_source_exists(&stdout, name))
                                .unwrap_or(false)
                    })
                    .unwrap_or(false);
                DependencyStatus {
                    name: name.to_string(),
                    present,
                    detail: if present {
                        "PipeWire/Pulse source exists".to_string()
                    } else {
                        format!("{name} source is missing; run --setup-virtual-mic")
                    },
                }
            }
            ProbeKind::KernelModule => {
                let present = std::path::Path::new("/sys/module/v4l2loopback").exists();
                DependencyStatus {
                    name: name.to_string(),
                    present,
                    detail: if present {
                        "kernel module loaded".to_string()
                    } else {
                        "kernel module is not loaded".to_string()
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    struct FakeProbe {
        present: HashSet<&'static str>,
    }

    impl DependencyProbe for FakeProbe {
        fn check(&self, name: &str, _kind: ProbeKind) -> DependencyStatus {
            DependencyStatus {
                name: name.to_string(),
                present: self.present.contains(name),
                detail: if self.present.contains(name) {
                    "ok".to_string()
                } else {
                    "missing".to_string()
                },
            }
        }
    }

    #[test]
    fn reports_all_ready_when_dependencies_are_present() {
        let probe = FakeProbe {
            present: [
                "v4l2loopback",
                "pipewire",
                "pactl",
                "gst-launch-1.0",
                virtual_mic::DEFAULT_SOURCE_NAME,
            ]
            .into(),
        };
        let report = DependencyReport::from_probe(&probe);
        assert!(report.virtual_camera_ready);
        assert!(report.virtual_microphone_ready);
    }

    #[test]
    fn maps_missing_dependencies_to_readiness_flags() {
        let probe = FakeProbe {
            present: ["pipewire"].into(),
        };
        let report = DependencyReport::from_probe(&probe);
        assert!(!report.virtual_camera_ready);
        assert!(!report.virtual_microphone_ready);
        assert_eq!(report.dependencies.len(), 5);
        assert_eq!(
            report.diagnostics(),
            vec![
                Diagnostic {
                    code: DiagnosticCode::MissingV4l2loopback,
                    severity: DiagnosticSeverity::Error,
                    message: "v4l2loopback is not loaded; create a virtual camera before starting a session."
                        .to_string(),
                },
                Diagnostic {
                    code: DiagnosticCode::MissingGstreamer,
                    severity: DiagnosticSeverity::Error,
                    message: "GStreamer RTP decode plugins are not available.".to_string(),
                },
            ]
        );
    }
}
