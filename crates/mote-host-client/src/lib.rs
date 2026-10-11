mod connection;
mod channel;
mod directory;
mod outbox;
mod state;

pub use connection::{ConnectionState, HostEventStream, HostTransport};
pub use channel::{command_worker, HostCommand, HostRequestTransport, HostUpdate};
pub use directory::{CredentialStore, HostDirectory, PairedHost};
pub use outbox::Outbox;
pub use state::HostSessionState;

use mote_protocol::{HostError, HostErrorCode, HostId};

fn require_host(expected: &HostId, actual: &HostId) -> Result<(), HostError> {
    if expected == actual {
        Ok(())
    } else {
        Err(HostError::new(
            HostErrorCode::IdentityChanged,
            "state or action belongs to another host",
        ))
    }
}
