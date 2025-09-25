mod config;
mod device;
mod virtual_pointer;

use async_std::task;
use calloop::{EventLoop, LoopHandle, RegistrationToken, generic::Generic};
use clap::Parser;
use common::{
    Profile,
    ipc::{self, IpcRequest, IpcResponse, Server},
};
use env_logger::Builder;
use evdev::{EventType, KeyCode};
use log::LevelFilter;
use std::{io::Write, os::fd::AsRawFd, path::PathBuf, sync::Arc};
use virtual_pointer::VirtualPointer;

struct Clicker {
    ipc: ipc::Ipc<Server>,
    config: config::Config,
    current_profile: Option<Profile>,
    pressed_keys: Vec<KeyCode>,
    registration_token: Option<RegistrationToken>,
    virtual_pointer: VirtualPointer,
    loop_handle: LoopHandle<'static, Self>,
}

impl Clicker {
    fn new(
        ipc: ipc::Ipc<Server>,
        config: config::Config,
        loop_handle: LoopHandle<'static, Self>,
    ) -> Self {
        let virtual_pointer = VirtualPointer::try_new().unwrap();

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

    let mut event_loop = EventLoop::try_new()?;

    let ipc = ipc::Ipc::server()?;

    let mut clicker = Clicker::new(ipc, config, event_loop.handle());

    let (executor, scheduler) = calloop::futures::executor()?;
    let (event_sender, event_receiver) = calloop::channel::channel();

    scheduler.schedule(async move {
        device::MouseDevices::try_new()
            .unwrap()
            .iter_mut()
            .for_each(|device| {
                let device = Arc::clone(&device);
                let event_sender = event_sender.clone();
                task::spawn(async move {
                    let mut device = device.lock().unwrap();
                    loop {
                        if let Ok(events) = device.fetch_events() {
                            for ev in events {
                                let EventType::KEY = ev.event_type() else {
                                    continue;
                                };

                                let code = KeyCode::new(ev.code());
                                let key_event = match ev.value() {
                                    0 => {
                                        log::debug!("Mouse released {code:?}");
                                        KeyEvent::Released(code)
                                    }
                                    1 => {
                                        log::debug!("Mouse pressed {code:?}");
                                        KeyEvent::Pressed(code)
                                    }
                                    _ => continue,
                                };

                                if let Err(e) = event_sender.send(key_event) {
                                    log::warn!("{e}");
                                }
                            }
                        }
                    }
                });
            });

        device::KeyboardDevices::try_new()
            .unwrap()
            .iter_mut()
            .for_each(|device| {
                let device = Arc::clone(&device);
                let event_sender = event_sender.clone();
                task::spawn(async move {
                    let mut device = device.lock().unwrap();
                    loop {
                        if let Ok(events) = device.fetch_events() {
                            for ev in events {
                                let EventType::KEY = ev.event_type() else {
                                    continue;
                                };

                                let code = KeyCode::new(ev.code());
                                let key_event = match ev.value() {
                                    0 => {
                                        log::debug!("Key released {code:?}");
                                        KeyEvent::Released(code)
                                    }
                                    1 => {
                                        log::debug!("Key pressed {code:?}");
                                        KeyEvent::Pressed(code)
                                    }
                                    _ => continue,
                                };

                                if let Err(e) = event_sender.send(key_event) {
                                    log::warn!("{e}");
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
                        current_profile.activation_keys,
                        current_profile.cps,
                        current_profile.jitter
                    );
                };

                match event {
                    KeyEvent::Pressed(key) => {

                        if state.pressed_keys.contains(&key) {
                        let all_keys_pressed = current_profile
                            .activation_keys
                            .iter()
                            .all(|profile_key| state.pressed_keys.contains(profile_key));
                            if all_keys_pressed {
                                log_profile_details(false);
                            }
                            state.pressed_keys.retain(|pressed_key| pressed_key != &key);
                        } else if key != current_profile.repeat_key {
                            state.pressed_keys.push(key);
                        }

                        let all_keys_pressed = current_profile
                            .activation_keys
                            .iter()
                            .all(|profile_key| state.pressed_keys.contains(profile_key));

                        if all_keys_pressed {
                            if current_profile.repeat_key == key {
                                log::info!(
                                    "Autoclicker started using profile '{}' with repeat key {:?} (CPS={:?}, jitter={:?})",
                                    current_profile.name,
                                    current_profile.repeat_key,
                                    current_profile.cps,
                                    current_profile.jitter
                                );
                                state.registration_token =
                                    state.virtual_pointer.schedule_clicks(&state.loop_handle);
                            } else {
                                log_profile_details(true);
                            }
                        }
                    }
                    KeyEvent::Released(key) => {
                        if key == current_profile.repeat_key {
                            if let Some(registration_token) = state.registration_token.take() {
                                log::info!(
                                    "Autoclicker stopped using profile '{}' with repeat key {:?} (CPS={:?}, jitter={:?})",
                                    current_profile.name,
                                    current_profile.repeat_key,
                                    current_profile.cps,
                                    current_profile.jitter
                                );
                                state.loop_handle.remove(registration_token);
                            }
                        }
                    }
                }
            },
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let source = unsafe {
        Generic::new(
            calloop::generic::FdWrapper::new(clicker.ipc.get_listener().as_raw_fd()),
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

    event_loop.run(None, &mut clicker, |_| {})?;

    Ok(())
}
