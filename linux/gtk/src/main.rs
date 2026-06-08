use std::cell::RefCell;
use std::fs;
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, CssProvider, DropDown, Entry, Image, Label, Orientation, ScrolledWindow,
    TextBuffer, TextView,
};
use libadwaita as adw;

const APP_ID: &str = "com.pocketlens.Launcher";
const DEFAULT_CONTROL_PORT: &str = "47650";
const DEFAULT_VIDEO_PORT: &str = "5004";
const DEFAULT_AUDIO_PORT: &str = "5006";

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum AppStatus {
    #[default]
    Idle,
    DevicesReady,
    ReceiverRunning,
}

#[derive(Default)]
struct AppState {
    receiver: Option<Child>,
    external_receiver: bool,
    status: AppStatus,
    pending_pairing_id: Option<String>,
}

#[derive(Debug, Clone)]
struct PendingPairingUi {
    pairing_id: String,
    device_name: String,
}

fn main() -> glib::ExitCode {
    let _ = libadwaita::init();
    load_css();
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    let state = Rc::new(RefCell::new(AppState::default()));
    let log = TextBuffer::new(None);

    let status_label = Label::builder()
        .label("Receiver stopped")
        .css_classes(["status-caption", "status-idle"])
        .halign(gtk::Align::Start)
        .build();

    let status_group = adw::PreferencesGroup::builder().title("Devices").build();

    let camera_row = adw::ActionRow::builder()
        .title("Camera")
        .subtitle("checking\u{2026}")
        .build();
    camera_row.add_prefix(&symbolic_icon("camera-video-symbolic"));

    let mic_row = adw::ActionRow::builder()
        .title("Microphone")
        .subtitle("checking\u{2026}")
        .build();
    mic_row.add_prefix(&symbolic_icon("audio-input-microphone-symbolic"));

    status_group.add(&camera_row);
    status_group.add(&mic_row);

    let setup_btn = Button::builder()
        .label("Setup All Devices")
        .css_classes(["suggested-action", "pill-button"])
        .hexpand(true)
        .build();
    let start_btn = Button::builder()
        .label("Start Receiver")
        .css_classes(["suggested-action", "pill-button"])
        .hexpand(true)
        .sensitive(false)
        .build();
    let stop_btn = Button::builder()
        .label("Stop Receiver")
        .css_classes(["destructive-action", "pill-button"])
        .hexpand(true)
        .visible(false)
        .build();

    let actions_box = GtkBox::new(Orientation::Vertical, 10);
    actions_box.add_css_class("actions-box");
    actions_box.append(&setup_btn);
    actions_box.append(&start_btn);
    actions_box.append(&stop_btn);

    let host_row = adw::EntryRow::builder()
        .title("Host")
        .text(default_host())
        .build();
    let camera_row_entry = adw::EntryRow::builder()
        .title("Camera device")
        .text("/dev/video10")
        .build();
    let port_row = adw::EntryRow::builder()
        .title("Control port")
        .text(DEFAULT_CONTROL_PORT)
        .build();

    let startup_row = adw::ActionRow::builder()
        .title("Startup at login")
        .subtitle(startup_service_status())
        .build();
    let startup_settings_enable_btn = Button::builder().label("Enable").build();
    let startup_settings_disable_btn = Button::builder().label("Disable").build();
    startup_row.add_suffix(&startup_settings_enable_btn);
    startup_row.add_suffix(&startup_settings_disable_btn);

    let settings_expander = adw::ExpanderRow::builder()
        .title("Settings")
        .show_enable_switch(false)
        .expanded(false)
        .build();
    settings_expander.add_row(&host_row);
    settings_expander.add_row(&camera_row_entry);
    settings_expander.add_row(&port_row);
    settings_expander.add_row(&startup_row);

    let adb_row = adw::ActionRow::builder()
        .title(if check_adb() {
            "adb found"
        } else {
            "adb not found"
        })
        .subtitle(if check_adb() {
            ""
        } else {
            "Install with: sudo apt install adb"
        })
        .build();
    adb_row.add_prefix(&symbolic_icon(if check_adb() {
        "emblem-ok-symbolic"
    } else {
        "dialog-warning-symbolic"
    }));

    let device_dropdown = DropDown::builder().build();
    let refresh_btn = Button::builder().label("Refresh").build();
    let device_row = adw::ActionRow::builder().title("Device").build();
    device_row.add_suffix(&refresh_btn);
    device_row.add_suffix(&device_dropdown);

    let install_apk_row = adw::ActionRow::builder()
        .title("Install APK to Device")
        .activatable(true)
        .sensitive(false)
        .build();
    let apk_result = Label::builder()
        .halign(gtk::Align::Start)
        .wrap(true)
        .css_classes(["dim-label", "apk-result"])
        .visible(false)
        .build();

    let devices = Rc::new(RefCell::new(Vec::<AdbDevice>::new()));

    refresh_btn.connect_clicked(glib::clone!(
        #[strong]
        devices,
        #[strong]
        device_dropdown,
        #[strong]
        install_apk_row,
        #[strong]
        apk_result,
        move |_| {
            apk_result.set_visible(false);
            match list_adb_devices() {
                Ok(list) => {
                    let strings: Vec<String> = list
                        .iter()
                        .map(|d| {
                            let model = d.model.as_deref().unwrap_or("unknown");
                            format!("{} ({}) — {}", d.serial, model, d.state)
                        })
                        .collect();
                    let store = gtk::StringList::new(
                        &strings.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                    );
                    device_dropdown.set_model(Some(&store));
                    install_apk_row.set_sensitive(!list.is_empty());
                    *devices.borrow_mut() = list;
                }
                Err(e) => {
                    apk_result.set_text(&format!("Error: {e}"));
                    apk_result.set_visible(true);
                    install_apk_row.set_sensitive(false);
                }
            }
        }
    ));

    install_apk_row.connect_activate(glib::clone!(
        #[strong]
        devices,
        #[strong]
        device_dropdown,
        #[strong]
        apk_result,
        move |_| {
            let list = devices.borrow();
            let idx = device_dropdown.selected() as usize;
            if let Some(device) = list.get(idx) {
                apk_result.set_text("Installing\u{2026}");
                apk_result.set_visible(true);
                match install_apk(&device.serial) {
                    Ok(msg) => {
                        apk_result.set_text(&format!("Success: {msg}"));
                    }
                    Err(e) => {
                        apk_result.set_text(&format!("Failed: {e}"));
                    }
                }
            }
        }
    ));

    let apk_expander = adw::ExpanderRow::builder()
        .title("Install APK")
        .subtitle("USB debugging, USB connection, and adb required")
        .show_enable_switch(false)
        .expanded(false)
        .build();
    apk_expander.add_row(&adb_row);
    apk_expander.add_row(&device_row);
    apk_expander.add_row(&install_apk_row);

    let apk_result_box = GtkBox::new(Orientation::Vertical, 0);
    apk_result_box.set_margin_start(12);
    apk_result_box.set_margin_end(12);
    apk_result_box.set_margin_bottom(8);
    apk_result_box.append(&apk_result);
    apk_expander.set_child(Some(&apk_result_box));

    let log_view = TextView::builder()
        .buffer(&log)
        .editable(false)
        .monospace(true)
        .css_classes(["log-view"])
        .vexpand(true)
        .build();
    let scroller = ScrolledWindow::builder()
        .child(&log_view)
        .min_content_height(120)
        .vexpand(true)
        .build();

    let log_expander = adw::ExpanderRow::builder()
        .title("Log")
        .show_enable_switch(false)
        .expanded(false)
        .build();
    log_expander.set_child(Some(&scroller));

    let advanced_group = adw::PreferencesGroup::builder().title("More").build();
    advanced_group.add(&settings_expander);
    advanced_group.add(&apk_expander);
    advanced_group.add(&log_expander);

    let content = GtkBox::new(Orientation::Vertical, 18);
    content.set_margin_top(6);
    content.set_margin_bottom(12);
    content.set_margin_start(6);
    content.set_margin_end(6);
    content.append(&status_label);
    content.append(&status_group);
    content.append(&actions_box);
    content.append(&advanced_group);

    let clamp = adw::Clamp::builder()
        .maximum_size(440)
        .tightening_threshold(360)
        .child(&content)
        .build();

    let scrolled = ScrolledWindow::builder()
        .child(&clamp)
        .vexpand(true)
        .build();

    let startup_banner = adw::Banner::builder()
        .title("Start PocketLens automatically at login?")
        .button_label("Enable")
        .revealed(!startup_prompt_seen())
        .build();

    let pairing_banner = adw::Banner::builder()
        .title("Pairing request")
        .revealed(false)
        .build();

    let pairing_bar = GtkBox::new(Orientation::Vertical, 8);
    pairing_bar.add_css_class("pairing-bar");
    pairing_bar.set_visible(false);
    let pairing_controls = GtkBox::new(Orientation::Horizontal, 8);
    pairing_controls.set_margin_start(18);
    pairing_controls.set_margin_end(18);
    pairing_controls.set_margin_bottom(12);
    let pairing_pin_entry = Entry::builder()
        .placeholder_text("Phone PIN")
        .max_length(8)
        .hexpand(true)
        .build();
    let pairing_approve_btn = Button::builder()
        .label("Approve")
        .css_classes(["suggested-action"])
        .build();
    pairing_controls.append(&pairing_pin_entry);
    pairing_controls.append(&pairing_approve_btn);
    pairing_bar.append(&pairing_controls);

    let header = adw::HeaderBar::new();
    let header_title = Label::builder()
        .label("PocketLens")
        .css_classes(["title"])
        .build();
    header.set_title_widget(Some(&header_title));

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.add_top_bar(&startup_banner);
    toolbar_view.add_top_bar(&pairing_banner);
    toolbar_view.add_top_bar(&pairing_bar);
    toolbar_view.set_content(Some(&scrolled));

    append_log(&log, "Ready. Android shows a PIN when pairing.");

    startup_banner.connect_button_clicked(glib::clone!(
        #[strong]
        startup_banner,
        #[strong]
        startup_row,
        #[strong]
        log,
        move |_| {
            match enable_startup_service() {
                Ok(()) => append_log(&log, "Startup service enabled."),
                Err(error) => append_log(&log, &format!("Startup service failed: {error}")),
            }
            mark_startup_prompt_seen();
            startup_banner.set_revealed(false);
            startup_row.set_subtitle(startup_service_status());
        }
    ));

    startup_banner.connect_notify_local(Some("revealed"), move |banner, _| {
        if !banner.is_revealed() {
            mark_startup_prompt_seen();
        }
    });

    startup_settings_enable_btn.connect_clicked(glib::clone!(
        #[strong]
        startup_row,
        #[strong]
        log,
        move |_| {
            match enable_startup_service() {
                Ok(()) => append_log(&log, "Startup service enabled."),
                Err(error) => append_log(&log, &format!("Startup service failed: {error}")),
            }
            startup_row.set_subtitle(startup_service_status());
        }
    ));

    startup_settings_disable_btn.connect_clicked(glib::clone!(
        #[strong]
        startup_row,
        #[strong]
        log,
        move |_| {
            match disable_startup_service() {
                Ok(()) => append_log(&log, "Startup service disabled."),
                Err(error) => append_log(&log, &format!("Disable startup failed: {error}")),
            }
            startup_row.set_subtitle(startup_service_status());
        }
    ));

    pairing_approve_btn.connect_clicked(glib::clone!(
        #[strong]
        state,
        #[strong]
        pairing_banner,
        #[strong]
        pairing_bar,
        #[strong]
        pairing_pin_entry,
        #[strong]
        port_row,
        #[strong]
        log,
        move |_| {
            let Some(pairing_id) = state.borrow().pending_pairing_id.clone() else {
                append_log(&log, "No pending pairing request.");
                return;
            };
            let pin = pairing_pin_entry.text().to_string();
            match approve_pairing(&port_row.text(), &pairing_id, &pin) {
                Ok(()) => {
                    append_log(&log, "Pairing approved.");
                    state.borrow_mut().pending_pairing_id = None;
                    pairing_pin_entry.set_text("");
                    pairing_banner.set_revealed(false);
                    pairing_bar.set_visible(false);
                }
                Err(error) => append_log(&log, &format!("Pairing approval failed: {error}")),
            }
        }
    ));

    setup_btn.connect_clicked(glib::clone!(
        #[strong]
        log,
        #[strong]
        state,
        #[strong]
        camera_row_entry,
        #[strong]
        status_label,
        #[strong]
        camera_row,
        #[strong]
        mic_row,
        #[strong]
        setup_btn,
        #[strong]
        start_btn,
        #[strong]
        stop_btn,
        move |_| {
            append_log(&log, "Setting up virtual camera\u{2026}");
            let camera_device = camera_row_entry.text().to_string();
            match run_receiver_command(&["--setup-camera", "--camera-device", &camera_device]) {
                Ok(_) => {
                    append_log(&log, "Virtual camera ready.");
                    set_device_status(&camera_row, "ready", Some(true));
                }
                Err(e) => {
                    append_log(&log, &format!("Camera setup failed: {e}"));
                    set_device_status(&camera_row, "failed", Some(false));
                }
            }

            append_log(&log, "Setting up virtual microphone\u{2026}");
            match run_receiver_command(&["--setup-virtual-mic"]) {
                Ok(_) => {
                    append_log(&log, "Virtual microphone ready.");
                    set_device_status(&mic_row, "ready", Some(true));
                }
                Err(e) => {
                    append_log(&log, &format!("Microphone setup failed: {e}"));
                    set_device_status(&mic_row, "failed", Some(false));
                }
            }

            state.borrow_mut().status = AppStatus::DevicesReady;
            update_status(
                &status_label,
                AppStatus::DevicesReady,
                &setup_btn,
                &start_btn,
                &stop_btn,
            );
            append_log(&log, "All devices set up. Start the receiver when ready.");
        }
    ));

    start_btn.connect_clicked(glib::clone!(
        #[strong]
        log,
        #[strong]
        state,
        #[strong]
        host_row,
        #[strong]
        camera_row_entry,
        #[strong]
        port_row,
        #[strong]
        status_label,
        #[strong]
        setup_btn,
        #[strong]
        start_btn,
        #[strong]
        stop_btn,
        move |_| {
            clear_exited_receiver(&log, &state);
            let control_port = port_row.text().to_string();

            if state.borrow_mut().receiver.is_some() {
                append_log(&log, "Receiver is already running.");
                return;
            }

            match receiver_status(&control_port) {
                Ok(()) => {
                    state.borrow_mut().status = AppStatus::ReceiverRunning;
                    state.borrow_mut().external_receiver = true;
                    update_status(
                        &status_label,
                        AppStatus::ReceiverRunning,
                        &setup_btn,
                        &start_btn,
                        &stop_btn,
                    );
                    append_log(
                        &log,
                        &format!("Using receiver already running on port {control_port}."),
                    );
                    return;
                }
                Err(error) if local_tcp_port_open(&control_port) => {
                    append_log(
                        &log,
                        &format!(
                            "Port {control_port} is already in use, but it is not a compatible PocketLens receiver: {error}"
                        ),
                    );
                    return;
                }
                Err(_) => {}
            }

            let receiver_bin = receiver_binary();
            let mut cmd = Command::new(receiver_bin);
            cmd.arg("--control-port")
                .arg(control_port.as_str())
                .arg("--video-port")
                .arg(DEFAULT_VIDEO_PORT)
                .arg("--audio-port")
                .arg(DEFAULT_AUDIO_PORT)
                .arg("--receiver-host")
                .arg(host_row.text().as_str())
                .arg("--camera-device")
                .arg(camera_row_entry.text().as_str())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            match cmd.spawn() {
                Ok(mut child) => {
                    match wait_for_receiver_ready(&control_port, &mut child) {
                        Ok(()) => {
                            {
                                let mut state = state.borrow_mut();
                                state.receiver = Some(child);
                                state.external_receiver = false;
                                state.status = AppStatus::ReceiverRunning;
                            }
                            update_status(
                                &status_label,
                                AppStatus::ReceiverRunning,
                                &setup_btn,
                                &start_btn,
                                &stop_btn,
                            );
                            append_log(&log, &format!("Receiver started on port {control_port}."));
                        }
                        Err(error) => {
                            let _ = child.kill();
                            let _ = child.wait();
                            let output = child_output(&mut child);
                            append_log(
                                &log,
                                &format!("Receiver failed to start: {error}{output}"),
                            );
                        }
                    }
                }
                Err(e) => append_log(&log, &format!("Failed to start receiver: {e}")),
            }
        }
    ));

    stop_btn.connect_clicked(glib::clone!(
        #[strong]
        log,
        #[strong]
        state,
        #[strong]
        status_label,
        #[strong]
        setup_btn,
        #[strong]
        start_btn,
        #[strong]
        stop_btn,
        move |_| {
            stop_receiver(&log, &state);
            state.borrow_mut().status = AppStatus::Idle;
            update_status(
                &status_label,
                AppStatus::Idle,
                &setup_btn,
                &start_btn,
                &stop_btn,
            );
        }
    ));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("PocketLens")
        .default_width(420)
        .default_height(580)
        .content(&toolbar_view)
        .build();

    window.connect_close_request(glib::clone!(
        #[strong]
        state,
        #[strong]
        log,
        move |_| {
            stop_receiver(&log, &state);
            glib::Propagation::Proceed
        }
    ));

    glib::timeout_add_seconds_local(
        2,
        glib::clone!(
            #[strong]
            state,
            #[strong]
            pairing_banner,
            #[strong]
            pairing_bar,
            #[strong]
            port_row,
            #[strong]
            log,
            move || {
                match pending_pairing(&port_row.text()) {
                    Ok(Some(request)) => {
                        let is_new = state.borrow().pending_pairing_id.as_deref()
                            != Some(request.pairing_id.as_str());
                        state.borrow_mut().pending_pairing_id = Some(request.pairing_id.clone());
                        pairing_banner.set_title(&format!(
                            "Pairing request from {}. Enter the PIN shown on the phone.",
                            request.device_name
                        ));
                        pairing_banner.set_revealed(true);
                        pairing_bar.set_visible(true);
                        if is_new {
                            append_log(
                                &log,
                                &format!(
                                    "Pairing request from {}. Type the phone PIN and approve.",
                                    request.device_name
                                ),
                            );
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        if state.borrow().status == AppStatus::ReceiverRunning {
                            append_log(&log, &format!("Pairing check failed: {error}"));
                        }
                    }
                }
                glib::ControlFlow::Continue
            }
        ),
    );

    window.present();
}

fn update_status(
    label: &Label,
    status: AppStatus,
    setup_btn: &Button,
    start_btn: &Button,
    stop_btn: &Button,
) {
    let (text, css) = match status {
        AppStatus::Idle => ("Receiver stopped", "status-idle"),
        AppStatus::DevicesReady => ("Devices ready \u{2014} start the receiver", "status-ready"),
        AppStatus::ReceiverRunning => (
            "Receiver active \u{2014} pair from Android",
            "status-active",
        ),
    };
    label.set_text(text);
    label.remove_css_class("status-idle");
    label.remove_css_class("status-ready");
    label.remove_css_class("status-active");
    label.remove_css_class("status-error");
    label.add_css_class(css);

    match status {
        AppStatus::Idle => {
            setup_btn.set_visible(true);
            start_btn.set_visible(true);
            stop_btn.set_visible(false);
        }
        AppStatus::DevicesReady => {
            setup_btn.set_visible(false);
            start_btn.set_visible(true);
            start_btn.set_sensitive(true);
            stop_btn.set_visible(false);
        }
        AppStatus::ReceiverRunning => {
            setup_btn.set_visible(false);
            start_btn.set_visible(false);
            stop_btn.set_visible(true);
        }
    }
}

fn set_device_status(row: &adw::ActionRow, subtitle: &str, success: Option<bool>) {
    row.set_subtitle(subtitle);
    row.remove_css_class("device-success");
    row.remove_css_class("device-error");
    match success {
        Some(true) => row.add_css_class("device-success"),
        Some(false) => row.add_css_class("device-error"),
        None => {}
    }
}

fn symbolic_icon(name: &str) -> Image {
    Image::builder()
        .icon_name(name)
        .pixel_size(16)
        .valign(gtk::Align::Center)
        .build()
}

fn stop_receiver(log: &TextBuffer, state: &Rc<RefCell<AppState>>) {
    let Some(mut child) = state.borrow_mut().receiver.take() else {
        if state.borrow().external_receiver {
            state.borrow_mut().external_receiver = false;
            state.borrow_mut().status = AppStatus::Idle;
            append_log(
                log,
                "Receiver is running outside this app; leaving that process alone.",
            );
        } else {
            append_log(log, "Receiver is not running.");
        }
        return;
    };
    match child.kill() {
        Ok(()) => {
            state.borrow_mut().external_receiver = false;
            state.borrow_mut().status = AppStatus::Idle;
            append_log(log, "Receiver stopped.");
        }
        Err(e) => append_log(log, &format!("Failed to stop receiver: {e}")),
    }
    let _ = child.wait();
}

fn receiver_binary() -> PathBuf {
    if let Ok(path) = std::env::current_exe()
        && let Some(dir) = path.parent()
    {
        let sibling = dir.join("pocketlens-receiver");
        if sibling.exists() {
            return sibling;
        }
    }
    PathBuf::from("pocketlens-receiver")
}

fn run_receiver_command(args: &[&str]) -> Result<String, String> {
    let output = Command::new(receiver_binary())
        .args(args)
        .output()
        .map_err(|error| format!("failed to run pocketlens-receiver: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[derive(Debug, Clone)]
struct AdbDevice {
    serial: String,
    state: String,
    model: Option<String>,
}

fn check_adb() -> bool {
    Command::new("sh")
        .arg("-c")
        .arg("command -v adb >/dev/null 2>&1")
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn list_adb_devices() -> Result<Vec<AdbDevice>, String> {
    let output = Command::new("adb")
        .args(["devices", "-l"])
        .output()
        .map_err(|error| format!("failed to run adb devices: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();
    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let model = parts
            .iter()
            .find(|part| part.starts_with("model:"))
            .map(|part| part.trim_start_matches("model:").to_string());
        devices.push(AdbDevice {
            serial: parts[0].to_string(),
            state: parts[1].to_string(),
            model,
        });
    }
    Ok(devices)
}

fn install_apk(device_serial: &str) -> Result<String, String> {
    let apk = bundled_apk_path().ok_or_else(|| "bundled APK not found".to_string())?;
    let output = Command::new("adb")
        .args(["-s", device_serial, "install", "-r"])
        .arg(apk)
        .output()
        .map_err(|error| format!("failed to run adb install: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() {
        Ok(format!("{stdout}\n{stderr}"))
    } else {
        Err(stderr.to_string())
    }
}

fn pending_pairing(control_port: &str) -> Result<Option<PendingPairingUi>, String> {
    let payload = curl_get(&format!(
        "http://127.0.0.1:{}/pair/pending",
        control_port.trim()
    ))?;
    parse_pending_pairing(&payload)
}

fn parse_pending_pairing(payload: &str) -> Result<Option<PendingPairingUi>, String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|error| format!("invalid pairing JSON: {error}"))?;
    let Some(request) = value
        .get("requests")
        .and_then(|requests| requests.as_array())
        .and_then(|requests| requests.first())
    else {
        return Ok(None);
    };
    let Some(pairing_id) = request.get("pairing_id").and_then(|value| value.as_str()) else {
        return Err("pending pairing response missing pairing_id".to_string());
    };
    let device_name = request
        .get("device_name")
        .and_then(|value| value.as_str())
        .unwrap_or("Android");
    Ok(Some(PendingPairingUi {
        pairing_id: pairing_id.to_string(),
        device_name: device_name.to_string(),
    }))
}

fn approve_pairing(control_port: &str, pairing_id: &str, pin: &str) -> Result<(), String> {
    let body = format!(
        "{{\"pairing_id\":\"{}\",\"pin\":\"{}\"}}",
        json_escape(pairing_id),
        json_escape(pin)
    );
    curl_post(
        &format!("http://127.0.0.1:{}/pair/approve", control_port.trim()),
        &body,
    )
    .map(|_| ())
}

fn curl_get(url: &str) -> Result<String, String> {
    let output = Command::new("curl")
        .args(["-fsS", url])
        .output()
        .map_err(|error| format!("failed to run curl: {error}"))?;
    command_output(output)
}

fn curl_post(url: &str, body: &str) -> Result<String, String> {
    let output = Command::new("curl")
        .args([
            "-fsS",
            "-H",
            "Content-Type: application/json",
            "-d",
            body,
            url,
        ])
        .output()
        .map_err(|error| format!("failed to run curl: {error}"))?;
    command_output(output)
}

fn command_output(output: std::process::Output) -> Result<String, String> {
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn receiver_status(control_port: &str) -> Result<(), String> {
    let payload = curl_get(&format!("http://127.0.0.1:{}/status", control_port.trim()))?;
    parse_receiver_status(&payload)
}

fn parse_receiver_status(payload: &str) -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|error| format!("invalid status JSON: {error}"))?;
    let protocol_version = value
        .get("protocol_version")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| "status response missing protocol_version".to_string())?;
    if protocol_version != 1 {
        return Err(format!("unsupported protocol version {protocol_version}"));
    }
    let capabilities = value
        .get("capabilities")
        .ok_or_else(|| "status response missing capabilities".to_string())?;
    if capabilities
        .get("secure_pairing")
        .and_then(|value| value.as_bool())
        != Some(true)
    {
        return Err(
            "receiver does not support secure pairing; reinstall/update PocketLens".to_string(),
        );
    }
    if capabilities
        .get("encrypted_rtp")
        .and_then(|value| value.as_bool())
        != Some(true)
    {
        return Err(
            "receiver does not support encrypted media; reinstall/update PocketLens".to_string(),
        );
    }
    Ok(())
}

fn local_tcp_port_open(control_port: &str) -> bool {
    let Ok(port) = control_port.trim().parse::<u16>() else {
        return false;
    };
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
}

fn wait_for_receiver_ready(control_port: &str, child: &mut Child) -> Result<(), String> {
    for _ in 0..20 {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("checking receiver process: {error}"))?
        {
            return Err(format!("process exited with {status}"));
        }
        if receiver_status(control_port).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "receiver did not become ready on port {}",
        control_port.trim()
    ))
}

fn child_output(child: &mut Child) -> String {
    let mut output = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        if !text.trim().is_empty() {
            output.push_str("\nStderr:\n");
            output.push_str(text.trim());
        }
    }
    if let Some(mut stdout) = child.stdout.take() {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        if !text.trim().is_empty() {
            output.push_str("\nStdout:\n");
            output.push_str(text.trim());
        }
    }
    output
}

fn clear_exited_receiver(log: &TextBuffer, state: &Rc<RefCell<AppState>>) {
    let status = {
        let mut state = state.borrow_mut();
        let Some(child) = state.receiver.as_mut() else {
            return;
        };
        child.try_wait()
    };
    match status {
        Ok(Some(status)) => {
            let mut child = state
                .borrow_mut()
                .receiver
                .take()
                .expect("receiver child exists");
            let output = child_output(&mut child);
            state.borrow_mut().status = AppStatus::Idle;
            append_log(log, &format!("Receiver exited with {status}.{output}"));
        }
        Ok(None) => {}
        Err(error) => append_log(log, &format!("Failed to check receiver process: {error}")),
    }
}

fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn bundled_apk_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [
        dir.join("../share/pocketlens/pocketlens.apk"),
        dir.join("share/pocketlens/pocketlens.apk"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

fn default_host() -> &'static str {
    "192.168.100.128"
}

fn startup_prompt_seen() -> bool {
    startup_seen_path().exists()
}

fn mark_startup_prompt_seen() {
    if let Some(parent) = startup_seen_path().parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(startup_seen_path(), "seen\n");
}

fn startup_seen_path() -> PathBuf {
    config_dir().join("startup-prompt.seen")
}

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pocketlens")
}

fn startup_service_status() -> &'static str {
    match Command::new("systemctl")
        .args(["--user", "is-enabled", "pocketlens-receiver.service"])
        .output()
    {
        Ok(output) if output.status.success() => "Startup: enabled",
        _ => "Startup: disabled",
    }
}

fn enable_startup_service() -> Result<(), String> {
    install_startup_unit()?;
    run_systemctl(&["--user", "daemon-reload"])?;
    run_systemctl(&["--user", "enable", "pocketlens-receiver.service"])?;
    Ok(())
}

fn disable_startup_service() -> Result<(), String> {
    run_systemctl(&["--user", "disable", "pocketlens-receiver.service"])
}

fn install_startup_unit() -> Result<(), String> {
    let unit_dir = systemd_user_dir();
    fs::create_dir_all(&unit_dir).map_err(|error| format!("creating systemd user dir: {error}"))?;
    let unit_path = unit_dir.join("pocketlens-receiver.service");
    let receiver = receiver_binary();
    let unit = format!(
        "[Unit]\n\
         Description=PocketLens receiver\n\
         After=network-online.target\n\n\
         [Service]\n\
         ExecStart={} --control-port {} --video-port {} --audio-port {} --camera-device /dev/video10\n\
         Restart=on-failure\n\n\
         [Install]\n\
         WantedBy=default.target\n",
        escape_systemd_path(&receiver),
        DEFAULT_CONTROL_PORT,
        DEFAULT_VIDEO_PORT,
        DEFAULT_AUDIO_PORT
    );
    fs::write(unit_path, unit).map_err(|error| format!("writing startup service: {error}"))
}

fn systemd_user_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("systemd/user")
}

fn run_systemctl(args: &[&str]) -> Result<(), String> {
    let output = Command::new("systemctl")
        .args(args)
        .output()
        .map_err(|error| format!("failed to run systemctl: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn escape_systemd_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace(' ', "\\x20")
}

fn append_log(log: &TextBuffer, message: &str) {
    let mut end = log.end_iter();
    log.insert(&mut end, message);
    log.insert(&mut end, "\n");
}

fn load_css() {
    let provider = CssProvider::new();
    provider.load_from_data(include_str!("style.css"));
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("could not get default display"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_first_pending_pairing_request() {
        let request = parse_pending_pairing(
            r#"{
                "requests": [
                    {
                        "pairing_id": "pair-1",
                        "device_name": "Pixel",
                        "requested_at_unix_ms": 1000,
                        "expires_in_seconds": 300
                    }
                ]
            }"#,
        )
        .unwrap()
        .unwrap();

        assert_eq!(request.pairing_id, "pair-1");
        assert_eq!(request.device_name, "Pixel");
    }

    #[test]
    fn empty_pending_pairing_response_has_no_request() {
        assert!(
            parse_pending_pairing(r#"{"requests":[]}"#)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn accepts_compatible_receiver_status() {
        parse_receiver_status(
            r#"{
                "receiver_name": "PocketLens Linux",
                "protocol_version": 1,
                "service_type": "_pocketlens._udp.local",
                "paired": false,
                "active_session": false,
                "capabilities": {
                    "video_codecs": ["h264"],
                    "audio_codecs": ["opus"],
                    "quality_presets": ["balanced"],
                    "adaptive_quality": false,
                    "secure_pairing": true,
                    "encrypted_rtp": true
                },
                "virtual_devices": {
                    "camera": {"name": "PocketLens", "ready": true, "backend": "v4l2loopback"},
                    "microphone": {"name": "PocketLens Microphone", "ready": true, "backend": "pipewire"}
                },
                "diagnostics": []
            }"#,
        )
        .unwrap();
    }

    #[test]
    fn rejects_receiver_without_secure_pairing() {
        let error = parse_receiver_status(
            r#"{
                "protocol_version": 1,
                "capabilities": {
                    "secure_pairing": false,
                    "encrypted_rtp": true
                }
            }"#,
        )
        .unwrap_err();

        assert!(error.contains("secure pairing"));
    }
}
