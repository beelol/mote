use crate::{ConnectionState, HostDirectory};
use mote_protocol::{ClientFrame, HostError, HostFrame, HostId};
use std::sync::mpsc::{Receiver, Sender};

#[derive(Debug, Clone)]
pub struct HostCommand {
    pub host_id: HostId,
    pub frame: ClientFrame,
}

#[derive(Debug, Clone)]
pub enum HostUpdate {
    Registered {
        host: crate::PairedHost,
    },
    Selected {
        host_id: HostId,
    },
    Frame {
        host_id: HostId,
        frame: HostFrame,
    },
    Connection {
        host_id: HostId,
        state: ConnectionState,
    },
    Failed {
        host_id: HostId,
        error: HostError,
    },
}

impl HostUpdate {
    pub fn host_id(&self) -> &HostId {
        match self {
            Self::Registered { host } => host.host_id(),
            Self::Selected { host_id }
            | Self::Frame { host_id, .. }
            | Self::Connection { host_id, .. }
            | Self::Failed { host_id, .. } => host_id,
        }
    }
}

impl HostDirectory {
    pub fn reduce_update(&mut self, update: HostUpdate) -> Result<(), HostError> {
        match update {
            HostUpdate::Selected { host_id } => self.select_host(&host_id),
            HostUpdate::Registered { host } => {
                if let Ok(existing) = self.host(host.host_id()) {
                    crate::require_host(existing.host_id(), host.host_id())
                } else {
                    self.register(host)
                }
            }
            HostUpdate::Frame { host_id, frame } => self.receive_frame(&host_id, frame),
            HostUpdate::Connection { host_id, state } => self.set_connection(&host_id, state),
            HostUpdate::Failed { host_id, error } => {
                self.state(&host_id)?;
                if error.code == mote_protocol::HostErrorCode::IdentityChanged {
                    self.set_connection(&host_id, ConnectionState::IdentityChanged)?;
                }
                Err(error)
            }
        }
    }
}

pub trait HostRequestTransport: Send + 'static {
    fn request(&mut self, command: HostCommand) -> Result<Vec<HostFrame>, HostError>;
}

pub fn command_worker(
    mut transport: impl HostRequestTransport,
    commands: Receiver<HostCommand>,
    updates: Sender<HostUpdate>,
    wake: impl Fn() + Send + 'static,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for command in commands {
            let host_id = command.host_id.clone();
            let result = transport.request(command);
            let frames = match result {
                Ok(frames) => frames
                    .into_iter()
                    .map(|frame| HostUpdate::Frame {
                        host_id: host_id.clone(),
                        frame,
                    })
                    .collect(),
                Err(error) => vec![HostUpdate::Failed { host_id, error }],
            };
            for update in frames {
                if updates.send(update).is_err() {
                    return;
                }
                wake();
            }
        }
    })
}
