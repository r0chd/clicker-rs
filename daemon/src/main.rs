mod config;
mod device;
mod virtual_pointer;

use async_std::task;
use calloop::{EventLoop, LoopHandle, RegistrationToken, generic::Generic};
use calloop_wayland_source::WaylandSource;
use clap::Parser;
use common::{
    Profile,
    ipc::{self, IpcRequest, IpcResponse, Server},
};
use env_logger::Builder;
use evdev::{EventType, KeyCode};
use log::LevelFilter;
use std::{io::Write, os::fd::AsRawFd, path::PathBuf, sync::Arc};
use std::{sync::LazyLock, time::Instant};
use virtual_pointer::VirtualPointer;
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalList, GlobalListContents, registry_queue_init},
    protocol::wl_registry,
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1, zwlr_virtual_pointer_v1,
};

static START: LazyLock<Instant> = LazyLock::new(Instant::now);

struct WlClicker {
    ipc: ipc::Ipc<Server>,
    config: config::Config,
    current_profile: Option<Profile>,
    pressed_keys: Vec<KeyCode>,
    registration_token: Option<RegistrationToken>,
    virtual_pointer: VirtualPointer,
    loop_handle: LoopHandle<'static, Self>,
}

impl WlClicker {
    fn new(
        globals: GlobalList,
        qh: QueueHandle<Self>,
        ipc: ipc::Ipc<Server>,
        config: config::Config,
        loop_handle: LoopHandle<'static, Self>,
    ) -> Self {
        let virtual_pointer = VirtualPointer::new(&globals, &qh);

        let current_profile = config
            .profiles
            .iter()
            .find(|profile| profile.name == "default")
            .cloned();

        Self {
            ipc,
            config,
            virtual_pointer,
            pressed_keys: Vec::new(),
            registration_token: None,
            current_profile,
            loop_handle,
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WlClicker {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: <wl_registry::WlRegistry as wayland_client::Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(WlClicker: zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1);
delegate_noop!(WlClicker: zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1);

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(long, value_enum, help = "Set the log level")]
    log_level: Option<LevelFilter>,

    #[arg(short, long, value_name = "FILE", help = "Path to the config file")]
    config: Option<PathBuf>,
}

#[derive(Debug)]
enum KeyEvent {
    Pressed(KeyCode),
    Released(KeyCode),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    Builder::new()
        .filter(Some("daemon"), cli.log_level.unwrap_or(LevelFilter::Info))
        .init();

    let config = config::Config::load(cli.config).unwrap();

    let conn = Connection::connect_to_env()?;
    let (globals, event_queue) = registry_queue_init(&conn)?;

    let mut event_loop = EventLoop::try_new()?;
    let qh = event_queue.handle();

    let ipc = ipc::Ipc::server()?;

    let mut wl_clicker = WlClicker::new(globals, qh, ipc, config, event_loop.handle());

    WaylandSource::new(conn, event_queue)
        .insert(event_loop.handle())
        .map_err(|e| anyhow::anyhow!("Failed to insert Wayland source: {}", e))?;

    let (executor, scheduler) = calloop::futures::executor()?;
    let (event_sender, event_receiver) = calloop::channel::channel();

    scheduler.schedule(async move {
        device::KeyboardDevices::try_new()
            .unwrap()
            .iter_mut()
            .for_each(|device| {
                let device = Arc::clone(&device);
                let event_sender = event_sender.clone();
                task::spawn(async move {
                    let mut device = device.lock().unwrap();
                    loop {
                        for ev in device.fetch_events().unwrap() {
                            if let EventType::KEY = ev.event_type() {
                                let code = KeyCode::new(ev.code());

                                match ev.value() {
                                    0 => {
                                        log::debug!("Key released {code:?}");
                                        if let Err(e) = event_sender.send(KeyEvent::Released(code))
                                        {
                                            log::warn!("{e}");
                                        }
                                    }
                                    1 => {
                                        log::debug!("Key pressed {code:?}");
                                        if let Err(e) = event_sender.send(KeyEvent::Pressed(code)) {
                                            log::warn!("{e}");
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                });
            });
    })?;
    event_loop
        .handle()
        .insert_source(executor, |_: (), _, _| ())
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    event_loop
        .handle()
        .insert_source(
            event_receiver,
            |event: calloop::channel::Event<KeyEvent>, _, state| {
                let calloop::channel::Event::Msg(event) = event else {
                    return;
                };
                let Some(current_profile) = state.current_profile.as_ref() else {
                    return;
                };

                let log_profile_details = |activated: bool| {
                    log::info!(
                        "Profile '{}' {} (toggle={}, keys={:?}, cps={:?}), jitter={:?}",
                        current_profile.name,
                        if activated {
                            "activated"
                        } else {
                            "deactivated"
                        },
                        current_profile.toggle,
                        current_profile.keys,
                        current_profile.cps,
                        current_profile.jitter
                    );
                };

                match event {
                    KeyEvent::Pressed(key) => {
                        if !state.pressed_keys.contains(&key) {
                            state.pressed_keys.push(key);
                        }
                        if current_profile.toggle {
                            let all_keys_pressed = current_profile
                                .keys
                                .iter()
                                .all(|profile_key| state.pressed_keys.contains(profile_key));

                            if all_keys_pressed {
                                if let Some(registration_token) = state.registration_token.take() {
                                    log_profile_details(false);
                                    state.loop_handle.remove(registration_token);
                                } else {
                                    log_profile_details(true);
                                    state.registration_token =
                                        state.virtual_pointer.schedule_clicks(
                                            current_profile.cps.clone(),
                                            &state.loop_handle,
                                        );
                                }
                            }
                        } else {
                            if current_profile.keys.contains(&key) {
                                if state.registration_token.is_none() {
                                    log_profile_details(true);
                                    state.registration_token =
                                        state.virtual_pointer.schedule_clicks(
                                            current_profile.cps.clone(),
                                            &state.loop_handle,
                                        );
                                }
                            }
                        }
                    }
                    KeyEvent::Released(key) => {
                        if let Some(pos) = state.pressed_keys.iter().position(|&k| k == key) {
                            state.pressed_keys.remove(pos);
                        }
                        if current_profile.toggle {
                            return;
                        }
                        let still_pressed = state
                            .pressed_keys
                            .iter()
                            .any(|k| current_profile.keys.contains(k));

                        if let Some(registration_token) = state.registration_token.take()
                            && !still_pressed
                        {
                            log_profile_details(false);
                            state.loop_handle.remove(registration_token);
                        } else if state.registration_token.is_none() && still_pressed {
                            log_profile_details(true);
                            state.registration_token = state
                                .virtual_pointer
                                .schedule_clicks(current_profile.cps.clone(), &state.loop_handle);
                        }
                    }
                }
            },
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let source = unsafe {
        Generic::new(
            calloop::generic::FdWrapper::new(wl_clicker.ipc.get_listener().as_raw_fd()),
            calloop::Interest {
                readable: true,
                writable: false,
            },
            calloop::Mode::Level,
        )
    };

    event_loop.handle().insert_source(source, |_, _, state| {
        let fd = state.ipc.accept_connection().as_raw_fd();

        let req = state.ipc.handle_stream_data(fd);
        let res = match req {
            Ok(IpcRequest::GetAllProfiles) => {
                log::info!("IPC: GetAllProfiles requested");
                IpcResponse::AllProfiles(state.config.profiles.clone())
            }
            Ok(IpcRequest::GetCurrentProfile) => {
                log::info!("IPC: GetCurrentProfile requested");
                match state.current_profile.as_ref() {
                    Some(profile) => {
                        log::info!("IPC: Current profile is '{}'", profile.name);
                        IpcResponse::Profile(profile.clone())
                    }
                    None => {
                        log::warn!("IPC: No current profile set");
                        IpcResponse::Error("No profile selected".to_string())
                    }
                }
            }
            Ok(IpcRequest::GetProfile { name }) => {
                log::info!("IPC: GetProfile requested for '{}'", name);
                match state
                    .config
                    .profiles
                    .iter()
                    .find(|profile| profile.name == name)
                {
                    Some(profile) => IpcResponse::Profile(profile.to_owned()),
                    None => {
                        log::warn!("IPC: Profile '{}' not found", name);
                        IpcResponse::Error(format!("Profile `{name}` doesn't exist"))
                    }
                }
            }
            Ok(IpcRequest::SwitchProfile { name }) => {
                log::info!("IPC: SwitchProfile requested to '{}'", name);
                match state
                    .config
                    .profiles
                    .iter()
                    .find(|profile| profile.name == name)
                {
                    Some(profile) => {
                        state.current_profile = Some(profile.clone());
                        log::info!("IPC: Switched to profile '{}'", profile.name);
                        IpcResponse::Ok
                    }
                    None => {
                        log::warn!("IPC: Failed to switch, profile '{}' not found", name);
                        IpcResponse::Error(format!("Profile `{name}` doesn't exist"))
                    }
                }
            }
            Err(err) => {
                log::error!("IPC: Failed to parse request: {err}");
                IpcResponse::Error(err.to_string())
            }
        };

        if let Ok(res) = serde_json::to_string(&res).map_err(|e| {
            log::error!("Failed to serialize output data: {e}");
            anyhow::anyhow!(e)
        }) {
            if let Some(conn) = state.ipc.get_mut(&fd) {
                if let Err(e) = conn
                    .write_all(format!("{res}\n").as_bytes())
                    .and_then(|_| conn.flush())
                {
                    log::error!("Stream write error: {e}");
                }
            }
        }

        Ok(calloop::PostAction::Continue)
    })?;

    event_loop.run(None, &mut wl_clicker, |_| {})?;

    Ok(())
}
