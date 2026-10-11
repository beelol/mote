use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
};

static REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn request_shutdown(_: libc::c_int) {
    REQUESTED.store(true, Ordering::Relaxed);
}

pub struct Shutdown {
    previous: Vec<(libc::c_int, libc::sigaction)>,
}

impl Shutdown {
    pub fn install() -> io::Result<Self> {
        REQUESTED.store(false, Ordering::Relaxed);
        let mut shutdown = Self {
            previous: Vec::new(),
        };
        for signal in [libc::SIGTERM, libc::SIGINT] {
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            let mut previous: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = request_shutdown as usize;
            if unsafe { libc::sigemptyset(&mut action.sa_mask) } != 0
                || unsafe { libc::sigaction(signal, &action, &mut previous) } != 0
            {
                return Err(io::Error::last_os_error());
            }
            shutdown.previous.push((signal, previous));
        }
        Ok(shutdown)
    }

    pub fn requested(&self) -> bool {
        REQUESTED.load(Ordering::Relaxed)
    }
}

impl Drop for Shutdown {
    fn drop(&mut self) {
        for (signal, previous) in self.previous.iter().rev() {
            unsafe {
                libc::sigaction(*signal, previous, std::ptr::null_mut());
            }
        }
    }
}
