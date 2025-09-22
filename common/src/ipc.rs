use crate::Profile;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    io::{Read, Write},
    marker::PhantomData,
    os::{
        fd::AsRawFd,
        unix::net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    sync::LazyLock,
};

static PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    let mut path = PathBuf::from(env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR not set"));
    path.push("mox/.wl-clicker-rs.sock");

    path
});

#[derive(Serialize, Deserialize, Debug)]
pub enum IpcRequest {
    GetProfile { name: String },
    GetAllProfiles,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum IpcResponse {
    Profile(Profile),
    AllProfiles(HashMap<String, Profile>),
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

    fn get_inner(&self) -> &ClientData {
        let IpcInner::Client(client_data) = &self.inner else {
            unreachable!();
        };

        client_data
    }

    pub fn get_stream(&self) -> &UnixStream {
        &self.get_inner().stream
    }

    pub fn request_profile(&self, name: &str) -> anyhow::Result<IpcResponse> {
        let req = IpcRequest::GetProfile {
            name: name.to_string(),
        };
        let mut stream = self.get_stream();

        serde_json::to_writer(stream, &req)?;
        stream.flush()?;

        let response: IpcResponse = serde_json::from_reader(stream)?;
        Ok(response)
    }

    pub fn request_all_profiles(&self) -> anyhow::Result<IpcResponse> {
        let req = IpcRequest::GetAllProfiles;
        let mut stream = self.get_stream();

        serde_json::to_writer(stream, &req)?;
        stream.flush()?;

        let response: IpcResponse = serde_json::from_reader(stream)?;
        Ok(response)
    }
}

impl Ipc<Server> {
    pub fn server() -> anyhow::Result<Self> {
        if let Ok(output) = std::process::Command::new("pidof").arg("moxpaper").output() {
            if output.status.success() {
                let pids = String::from_utf8_lossy(&output.stdout);
                if pids.split_whitespace().count() > 1 {
                    return Err(anyhow::anyhow!("moxpaper is already running"));
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

    pub fn handle_stream_data(&mut self, fd: &i32) -> anyhow::Result<IpcRequest> {
        let mut buffer = Vec::new();

        if let Some(stream) = self.get_mut(fd) {
            match stream.read_to_end(&mut buffer) {
                Ok(0) => {
                    self.remove_connection(fd);
                    Err(anyhow::anyhow!("Connection removed"))
                }
                Ok(n) => {
                    let data = &buffer[..n];
                    Ok(serde_json::from_slice::<IpcRequest>(data)?)
                }
                Err(e) => {
                    self.remove_connection(fd);
                    Err(anyhow::anyhow!(e))
                }
            }
        } else {
            Err(anyhow::anyhow!(""))
        }
    }
}
