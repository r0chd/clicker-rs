mod config;

use calloop::{EventLoop, generic::Generic};
use calloop_wayland_source::WaylandSource;
use clap::Parser;
use common::{
    Profile,
    ipc::{self, IpcRequest, IpcResponse, Server},
};
use env_logger::Builder;
use log::LevelFilter;
use std::{io::Write, os::fd::AsRawFd, path::PathBuf};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalList, GlobalListContents, registry_queue_init},
    protocol::wl_registry,
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1, zwlr_virtual_pointer_v1,
};

struct WlClicker {
    qh: QueueHandle<Self>,
    ipc: ipc::Ipc<Server>,
    config: config::Config,
    current_profile: Option<Profile>,
    virtual_pointer: zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
}

impl WlClicker {
    fn new(
        globals: GlobalList,
        qh: QueueHandle<Self>,
        ipc: ipc::Ipc<Server>,
        config: config::Config,
    ) -> Self {
        let virtual_pointer_manager = globals
            .bind::<zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1, _, _>(
                &qh,
                1..=2,
                (),
            )
            .expect("Compositor doesn't support zwlr_virtual_pointer_v1");
        let virtual_pointer = virtual_pointer_manager.create_virtual_pointer(None, &qh, ());

        Self {
            qh,
            ipc,
            config,
            virtual_pointer,
            current_profile: None,
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

#[derive(clap::ValueEnum, Clone, Debug)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(
        long,
        value_enum,
        default_value_t = LogLevel::Info,
        help = "Set the log level"
    )]
    log_level: LogLevel,

    #[arg(short, long, value_name = "FILE", help = "Path to the config file")]
    config: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let log_level = match cli.log_level {
        LogLevel::Error => LevelFilter::Error,
        LogLevel::Warn => LevelFilter::Warn,
        LogLevel::Info => LevelFilter::Info,
        LogLevel::Debug => LevelFilter::Debug,
        LogLevel::Trace => LevelFilter::Trace,
    };

    Builder::new().filter(Some("daemon"), log_level).init();

    let config = config::Config::load(cli.config).unwrap_or_default();

    let conn = Connection::connect_to_env()?;
    let (globals, event_queue) = registry_queue_init(&conn)?;

    let qh = event_queue.handle();

    let ipc = ipc::Ipc::server()?;

    let mut wl_clicker = WlClicker::new(globals, qh, ipc, config);

    let mut event_loop = EventLoop::try_new()?;

    WaylandSource::new(conn, event_queue)
        .insert(event_loop.handle())
        .map_err(|e| anyhow::anyhow!("Failed to insert Wayland source: {}", e))?;

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
        log::info!("Connection added");

        let req = state.ipc.handle_stream_data(fd);

        let res = match req {
            Ok(IpcRequest::GetAllProfiles) => {
                IpcResponse::AllProfiles(state.config.profiles.clone())
            }
            Ok(IpcRequest::GetCurrentProfile) => match state.current_profile.as_ref() {
                Some(profile) => IpcResponse::Profile(profile.clone()),
                None => IpcResponse::Error("No profile selected".to_string()),
            },
            Ok(IpcRequest::GetProfile { name }) => match state.config.profiles.get(&name) {
                Some(profile) => IpcResponse::Profile(profile.to_owned()),
                None => IpcResponse::Error(format!("Profile `{name}` doesn't exist")),
            },
            Ok(IpcRequest::SwitchProfile { name }) => match state.config.profiles.get(&name) {
                Some(profile) => {
                    state.current_profile = Some(profile.clone());
                    IpcResponse::Ok
                }
                None => IpcResponse::Error(format!("Profile `{name}` doesn't exist")),
            },
            Err(err) => IpcResponse::Error(err.to_string()),
        };

        let res = serde_json::to_string(&res).map_err(|e| {
            log::error!("Failed to serialize output data: {e}");
            anyhow::anyhow!(e)
        });

        if let Ok(res) = res {
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
    drop(event_loop);

    Ok(())
}
