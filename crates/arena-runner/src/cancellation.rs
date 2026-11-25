use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

static CANCELLED: AtomicBool = AtomicBool::new(false);

pub fn check() -> io::Result<()> {
    if CANCELLED.load(Ordering::Relaxed) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "match interrupted",
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn install() -> io::Result<()> {
    extern "C" fn mark_cancelled(_: libc::c_int) {
        CANCELLED.store(true, Ordering::Relaxed);
    }

    // SAFETY: the signal handler only stores to a lock-free atomic flag.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = mark_cancelled as *const () as libc::sighandler_t;
        if libc::sigemptyset(&mut action.sa_mask) == -1 {
            return Err(io::Error::last_os_error());
        }
        for signal in [libc::SIGINT, libc::SIGTERM] {
            if libc::sigaction(signal, &action, std::ptr::null_mut()) == -1 {
                return Err(io::Error::last_os_error());
            }
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn install() -> io::Result<()> {
    Ok(())
}
