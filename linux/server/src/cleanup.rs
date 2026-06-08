use anyhow::Result;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

const TARGET_NAMES: &[&str] = &["pocketlens-receiver", "gst-launch-1.0"];

pub fn cleanup_processes() -> Result<()> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(sysinfo::UpdateKind::Always),
    );

    let pids: Vec<_> = sys
        .processes()
        .iter()
        .filter(|(_, proc)| {
            let name = proc.name().to_str().unwrap_or("");
            TARGET_NAMES.contains(&name)
        })
        .map(|(pid, _)| *pid)
        .collect();

    let count = pids.len();
    for pid in &pids {
        if let Some(proc) = sys.process(*pid) {
            proc.kill();
        }
    }

    if count > 0 {
        println!("stopped {count} process(es)");
    } else {
        println!("no stale processes found");
    }
    Ok(())
}
