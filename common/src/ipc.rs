use crate::Profile;
use nix::unistd::{Group, Uid, chown};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    io::{BufRead, BufReader, BufWriter, Write},
    marker::PhantomData,
    os::{
        fd::AsRawFd,
        unix::{
            fs::PermissionsExt,
            net::{UnixListener, UnixStream},
        },
    },
    path::PathBuf,
    sync::LazyLock,
};

static PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    let mut path = PathBuf::from(env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR not set"));
    path.push("clicker-rs/.clicker-rs.sock");

    path
});

#[derive(Serialize, Deserialize, Debug)]
pub enum IpcRequest {
    SwitchProfile { name: String },
    GetProfile { name: String },
    GetCurrentProfile,
    GetAllProfiles,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum IpcResponse {
    Profile(Profile),
    AllProfiles(Vec<Profile>),
    Ok,
    Error(String),
}

pub struct Client;
pub struct Server;

pub struct Ipc<T> {
    phantom: PhantomData<T>,
    inner: IpcInner,
}

struct ServerData {
    listener: UnixListener,
    connections: HashMap<i32, UnixStream>,
}

struct ClientData {
    stream: UnixStream,
}

enum IpcInner {
    Server(ServerData),
    Client(ClientData),
}

impl Ipc<Client> {
    pub fn connect() -> anyhow::Result<Self> {
        let stream = UnixStream::connect(&*PATH)?;

        Ok(Self {
            inner: IpcInner::Client(ClientData { stream }),
            phantom: PhantomData,
        })
    }

    fn get_inner(&mut self) -> &mut ClientData {
        let IpcInner::Client(client_data) = &mut self.inner else {
            unreachable!();
        };

        client_data
    }

    fn send_request_and_receive_response(
        &mut self,
        request: IpcRequest,
    ) -> anyhow::Result<IpcResponse> {
        let inner = self.get_inner();

        // Send request as JSON line
        let mut writer = BufWriter::new(&inner.stream);
        let request_json = serde_json::to_string(&request)?;
        writeln!(writer, "{}", request_json)?;
        writer.flush()?;

        // Read response as JSON line
        let mut reader = BufReader::new(&inner.stream);
        let mut response_line = String::new();
        reader.read_line(&mut response_line)?;

        let response: IpcResponse = serde_json::from_str(response_line.trim())?;
        Ok(response)
    }

    pub fn request_profile(&mut self, name: String) -> anyhow::Result<IpcResponse> {
        self.send_request_and_receive_response(IpcRequest::GetProfile { name })
    }

    pub fn request_all_profiles(&mut self) -> anyhow::Result<IpcResponse> {
        self.send_request_and_receive_response(IpcRequest::GetAllProfiles)
    }

    pub fn request_current_profile(&mut self) -> anyhow::Result<IpcResponse> {
        self.send_request_and_receive_response(IpcRequest::GetCurrentProfile)
    }

    pub fn switch_profile(&mut self, name: String) -> anyhow::Result<IpcResponse> {
        self.send_request_and_receive_response(IpcRequest::SwitchProfile { name })
    }
}

impl Ipc<Server> {
    pub fn server() -> anyhow::Result<Self> {
        if let Ok(output) = std::process::Command::new("pidof").arg("clickerd").output() {
            if output.status.success() {
                let pids = String::from_utf8_lossy(&output.stdout);
                if pids.split_whitespace().count() > 1 {
                    return Err(anyhow::anyhow!("clicker-rs is already running"));
                }
            }
        }

        if !PATH.exists() {
            std::fs::create_dir_all(
                PATH.parent()
                    .ok_or(anyhow::anyhow!("Parent of {:#?} not found", PATH))?,
            )?;
        } else {
            std::fs::remove_file(&*PATH)?;
        }

        let listener = UnixListener::bind(&*PATH)?;

        let group = Group::from_name("clicker")?
            .ok_or_else(|| anyhow::anyhow!("Group 'clicker' not found"))?;

        let mut perms = std::fs::metadata(&*PATH)?.permissions();
        perms.set_mode(0o660);
        std::fs::set_permissions(&*PATH, perms)?;

        chown(&*PATH, Some(Uid::from_raw(0)), Some(group.gid))?;

        Ok(Self {
            inner: IpcInner::Server(ServerData {
                listener,
                connections: HashMap::new(),
            }),
            phantom: PhantomData,
        })
    }

    fn get_inner(&self) -> &ServerData {
        let IpcInner::Server(server_data) = &self.inner else {
            unreachable!();
        };

        server_data
    }

    fn get_inner_mut(&mut self) -> &mut ServerData {
        let IpcInner::Server(server_data) = &mut self.inner else {
            unreachable!();
        };

        server_data
    }

    pub fn accept_connection(&mut self) -> &UnixStream {
        let inner = self.get_inner_mut();

        let (stream, _) = inner
            .listener
            .accept()
            .expect("Failed to accept connection");
        let fd = stream.as_raw_fd();
        inner.connections.entry(fd).or_insert(stream)
    }

    pub fn remove_connection(&mut self, fd: &i32) {
        let inner = self.get_inner_mut();
        _ = inner.connections.remove(fd);
    }

    pub fn get_listener(&self) -> &UnixListener {
        let inner = self.get_inner();
        &inner.listener
    }

    pub fn get_mut(&mut self, fd: &i32) -> Option<&mut UnixStream> {
        let inner = self.get_inner_mut();
        inner.connections.get_mut(fd)
    }

    pub fn handle_stream_data(&mut self, fd: i32) -> anyhow::Result<IpcRequest> {
        if let Some(stream) = self.get_mut(&fd) {
            let mut reader = BufReader::new(stream);
            let mut line = String::new();

            match reader.read_line(&mut line) {
                Ok(0) => {
                    self.remove_connection(&fd);
                    Err(anyhow::anyhow!("Connection closed"))
                }
                Ok(_) => {
                    let request: IpcRequest = serde_json::from_str(line.trim())?;
                    Ok(request)
                }
                Err(e) => {
                    self.remove_connection(&fd);
                    Err(anyhow::anyhow!(e))
                }
            }
        } else {
            Err(anyhow::anyhow!("Connection not found"))
        }
    }
}
