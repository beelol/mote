use mote_gateway::ClientCredential;
use mote_host_client::CredentialStore;
use mote_protocol::HostId;
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredCredential {
    host_public: [u8; 32],
    private: [u8; 32],
    counter: i64,
}

impl StoredCredential {
    pub fn paired(credential: &ClientCredential) -> Self {
        Self {
            host_public: *credential.host_public_key(),
            private: *credential.private_key_for_secure_storage(),
            counter: 1,
        }
    }

    pub fn restore(&self, host: &HostId) -> io::Result<ClientCredential> {
        if HostId::from_public_key(&self.host_public) != *host || self.counter < 1 {
            return Err(io::Error::other("credential belongs to another host"));
        }
        ClientCredential::restore(self.host_public, self.private).map_err(io::Error::other)
    }

    pub fn reserve_counter(&mut self) -> io::Result<i64> {
        self.counter = self
            .counter
            .checked_add(1)
            .ok_or_else(|| io::Error::other("session counter exhausted"))?;
        Ok(self.counter)
    }
}

pub struct KeychainStore;

impl CredentialStore for KeychainStore {
    type Credential = StoredCredential;
    fn load(&self, host: &HostId) -> io::Result<Option<StoredCredential>> {
        #[cfg(target_os = "ios")]
        {
            let account = std::ffi::CString::new(host.as_str())?;
            let mut bytes = [0u8; 4096];
            let count = unsafe {
                mote_ios_keychain_load(account.as_ptr(), bytes.as_mut_ptr(), bytes.len())
            };
            if count == -25300 {
                return Ok(None);
            }
            if count < 0 {
                return Err(io::Error::other(format!("Keychain read failed: {count}")));
            }
            let stored: StoredCredential =
                serde_json::from_slice(&bytes[..count as usize]).map_err(io::Error::other)?;
            stored.restore(host)?;
            Ok(Some(stored))
        }
        #[cfg(not(target_os = "ios"))]
        {
            let _ = host;
            Err(io::Error::other("iOS Keychain requires the native app"))
        }
    }
    fn save(&mut self, host: &HostId, value: &StoredCredential) -> io::Result<()> {
        value.restore(host)?;
        #[cfg(target_os = "ios")]
        {
            let account = std::ffi::CString::new(host.as_str())?;
            let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
            status(unsafe { mote_ios_keychain_save(account.as_ptr(), bytes.as_ptr(), bytes.len()) })
        }
        #[cfg(not(target_os = "ios"))]
        {
            Err(io::Error::other("iOS Keychain requires the native app"))
        }
    }
    fn delete(&mut self, host: &HostId) -> io::Result<()> {
        #[cfg(target_os = "ios")]
        {
            let account = std::ffi::CString::new(host.as_str())?;
            status(unsafe { mote_ios_keychain_delete(account.as_ptr()) })
        }
        #[cfg(not(target_os = "ios"))]
        {
            let _ = host;
            Err(io::Error::other("iOS Keychain requires the native app"))
        }
    }
}

#[cfg(target_os = "ios")]
fn status(value: i32) -> io::Result<()> {
    if value == 0 || value == -25300 {
        Ok(())
    } else {
        Err(io::Error::other(format!("Keychain write failed: {value}")))
    }
}

#[cfg(target_os = "ios")]
extern "C" {
    fn mote_ios_keychain_load(
        account: *const std::ffi::c_char,
        output: *mut u8,
        capacity: usize,
    ) -> isize;
    fn mote_ios_keychain_save(
        account: *const std::ffi::c_char,
        bytes: *const u8,
        length: usize,
    ) -> i32;
    fn mote_ios_keychain_delete(account: *const std::ffi::c_char) -> i32;
}
