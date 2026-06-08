use std::net::SocketAddr;

use acamera_server::{
    camera, check_deps, cleanup, config::Cli, control, diagnostics, discovery, install, mdns,
    virtual_mic,
};
use anyhow::Context;
use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let config = cli.to_config()?;
    let virtual_mic_plan = virtual_mic::VirtualMicPlan {
        sink_name: config.microphone_sink_name.clone(),
        ..virtual_mic::VirtualMicPlan::default()
    };

    if cli.setup_virtual_mic {
        virtual_mic::setup_virtual_microphone(&virtual_mic_plan)?;
        println!(
            "created PipeWire/Pulse source '{}' via sink '{}'",
            virtual_mic_plan.source_description, virtual_mic_plan.sink_name
        );
        return Ok(());
    }

    if cli.remove_virtual_mic {
        virtual_mic::remove_virtual_microphone(&virtual_mic_plan)?;
        println!(
            "removed PipeWire/Pulse modules for source '{}' where present",
            virtual_mic_plan.source_description
        );
        return Ok(());
    }

    if cli.diagnose {
        let report = diagnostics::DependencyReport::probe_system();
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    if cli.check_deps {
        print!("{}", check_deps::check_deps());
        return Ok(());
    }

    if cli.cleanup {
        cleanup::cleanup_processes()?;
        return Ok(());
    }

    if cli.setup_camera {
        camera::setup_camera(&cli.camera_device)?;
        println!("virtual camera ready at {}", cli.camera_device);
        return Ok(());
    }

    if cli.remove_camera {
        camera::remove_camera()?;
        println!("virtual camera removed");
        return Ok(());
    }

    if cli.install {
        let prefix_str = shellexpand::tilde(&cli.prefix);
        let prefix = std::path::PathBuf::from(prefix_str.as_ref());
        install::install_app(&prefix)?;
        return Ok(());
    }

    if cli.install_apk {
        if !install::check_adb() {
            anyhow::bail!("adb not found. Install it: sudo apt install adb");
        }
        let devices = install::list_adb_devices()?;
        if devices.is_empty() {
            anyhow::bail!("no devices connected. Connect via USB and enable USB debugging.");
        }
        for device in &devices {
            let model = device.model.as_deref().unwrap_or("unknown");
            println!("{} ({}) — {}", device.serial, model, device.state);
        }
        let device = &devices[0];
        let result = install::install_apk(&device.serial)?;
        println!("{result}");
        return Ok(());
    }

    let state = control::AppState::new(config.clone());
    let app = control::router(state);
    let bind: SocketAddr = SocketAddr::new(config.bind_address, config.control_port);
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("failed to bind control server to {bind}"))?;

    let mut mdns_advertiser = match mdns::SystemMdnsAdvertiser::new() {
        Ok(advertiser) => Some(advertiser),
        Err(error) => {
            tracing::warn!(%error, "failed to start ACamera mDNS advertiser");
            None
        }
    };
    if let Some(advertiser) = mdns_advertiser.as_mut() {
        mdns::advertise_receiver(advertiser, &config.receiver_name, config.control_port);
        tracing::info!(
            service_type = acamera_server::protocol::SERVICE_TYPE,
            receiver_name = %config.receiver_name,
            control_port = config.control_port,
            "advertising ACamera receiver over mDNS"
        );
    }
    let discovery_responder = discovery::spawn_udp_responder(config.clone());

    tracing::info!(%bind, "starting ACamera receiver control server");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    discovery_responder.abort();
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::warn!(%error, "failed to install ctrl-c handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::warn!(%error, "failed to install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("stopping ACamera receiver control server");
}
