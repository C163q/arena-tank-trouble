use std::ffi::{CStr, CString, c_void};
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use arena_core::Action;

use crate::controller::{Controller, Decision, Status};
use crate::matchplay::Observation;

const ABI_VERSION: u32 = 1;
const INIT_BUDGET: Duration = Duration::from_secs(1);
const MAX_OBSERVATION: usize = 1024 * 1024;

pub struct Native {
    name: String,
    library: PathBuf,
    process: Option<Process>,
}

impl Native {
    pub fn new(name: String, library: PathBuf) -> io::Result<Self> {
        let process = Process::launch(&library)?;
        Ok(Self {
            name,
            library,
            process: Some(process),
        })
    }

    fn try_decide(&mut self, observation: &Observation<'_>) -> io::Result<Action> {
        if self.process.is_none() {
            self.process = Some(Process::launch(&self.library)?);
        }
        let bytes = serde_json::to_vec(observation)?;
        if bytes.len() > MAX_OBSERVATION {
            return Err(io::Error::other("observation exceeds IPC limit"));
        }
        let process = self.process.as_ref().expect("worker started");
        let budget = Duration::from_millis(u64::from(observation.decision_budget_ms));
        process.decide(&bytes, budget)
    }
}

impl Controller for Native {
    fn name(&self) -> &str {
        &self.name
    }

    fn decide(&mut self, obs: &Observation<'_>) -> Decision {
        match self.try_decide(obs) {
            Ok(action) => Decision::ok(action),
            Err(error) => {
                self.process = None;
                let status = if error.kind() == io::ErrorKind::TimedOut {
                    Status::Timeout
                } else {
                    Status::Error
                };
                Decision::failed(status, error)
            }
        }
    }

    fn eliminate(&mut self) {
        self.process = None;
    }
}

struct Process {
    child: Child,
    channel: File,
}

impl Process {
    fn launch(library: &Path) -> io::Result<Self> {
        let (parent, child) = UnixStream::pair()?;
        parent.set_nonblocking(true)?;
        let child = Command::new(std::env::current_exe()?)
            .arg("--worker")
            .arg(library)
            .arg(std::process::id().to_string())
            .stdin(Stdio::from(OwnedFd::from(child)))
            .process_group(0)
            .spawn()?;
        let worker = Self {
            child,
            channel: File::from(OwnedFd::from(parent)),
        };
        let deadline = Instant::now() + INIT_BUDGET;
        if worker.read_word(deadline)? != ABI_VERSION {
            return Err(io::Error::other("unsupported bot ABI version"));
        }
        worker.pause(deadline)?;
        Ok(worker)
    }

    fn decide(&self, data: &[u8], budget: Duration) -> io::Result<Action> {
        let deadline = Instant::now() + budget;
        self.signal(libc::SIGCONT)?;
        let length = u32::try_from(data.len())
            .expect("bounded observation")
            .to_le_bytes();
        self.write(&length, deadline)?;
        self.write(data, deadline)?;
        let action = decode(self.read_word(deadline)?)?;
        self.pause(deadline)?;
        Ok(action)
    }

    fn signal(&self, signal: libc::c_int) -> io::Result<()> {
        // SAFETY: launch places the owned child in its own process group.
        if unsafe { libc::killpg(self.child.id() as libc::pid_t, signal) } == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn pause(&self, deadline: Instant) -> io::Result<()> {
        self.signal(libc::SIGSTOP)?;
        loop {
            time_left(deadline)?;
            // SAFETY: waitid writes a siginfo_t and WNOWAIT leaves the child waitable.
            let mut status: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let result = unsafe {
                libc::waitid(
                    libc::P_PID,
                    self.child.id(),
                    &mut status,
                    libc::WSTOPPED | libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if result == -1 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            // SAFETY: si_pid is from waitid's child-state result.
            if unsafe { status.si_pid() } != 0 {
                return if status.si_code == libc::CLD_STOPPED {
                    time_left(deadline).map(|_| ())
                } else {
                    Err(io::Error::other("bot worker exited"))
                };
            }
            std::thread::yield_now();
        }
    }

    fn read_word(&self, deadline: Instant) -> io::Result<u32> {
        let mut bytes = [0; 4];
        self.transfer(bytes.len(), libc::POLLIN, deadline, |at| {
            (&self.channel).read(&mut bytes[at..])
        })?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn write(&self, bytes: &[u8], deadline: Instant) -> io::Result<()> {
        self.transfer(bytes.len(), libc::POLLOUT, deadline, |at| {
            (&self.channel).write(&bytes[at..])
        })
    }

    fn transfer(
        &self,
        length: usize,
        event: libc::c_short,
        deadline: Instant,
        mut operation: impl FnMut(usize) -> io::Result<usize>,
    ) -> io::Result<()> {
        let mut offset = 0;
        while offset < length {
            let remain = time_left(deadline)?;
            match operation(offset) {
                Ok(0) => return Err(io::Error::other("bot worker closed the channel")),
                Ok(bytes) => offset += bytes,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    let mut ready = libc::pollfd {
                        fd: self.channel.as_raw_fd(),
                        events: event,
                        revents: 0,
                    };
                    let timeout = remain.as_micros().div_ceil(1000) as libc::c_int;
                    // SAFETY: ready points to one pollfd valid for the call.
                    if unsafe { libc::poll(&mut ready, 1, timeout) } == -1 {
                        let error = io::Error::last_os_error();
                        if error.kind() != io::ErrorKind::Interrupted {
                            return Err(error);
                        }
                    }
                }
                Err(error) => return Err(error),
            }
        }
        time_left(deadline).map(|_| ())
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.signal(libc::SIGKILL);
        let _ = self.child.wait();
    }
}

fn time_left(deadline: Instant) -> io::Result<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "bot exceeded its deadline",
        ));
    }
    Ok(remaining)
}

fn decode(bits: u32) -> io::Result<Action> {
    if bits & !31 != 0 {
        return Err(io::Error::other("bot returned unsupported action bits"));
    }
    Ok(Action {
        forward: bits & 1 != 0,
        reverse: bits & 2 != 0,
        turn_left: bits & 4 != 0,
        turn_right: bits & 8 != 0,
        fire: bits & 16 != 0,
    })
}

type Version = unsafe extern "C" fn() -> u32;
type Decide = unsafe extern "C" fn(*const u8, usize) -> u32;

struct Library(*mut c_void);

impl Library {
    fn open(path: &Path) -> io::Result<Self> {
        let path = CString::new(path.as_os_str().as_bytes())?;
        // SAFETY: path is a terminated, live C string; the worker owns the library.
        let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            // SAFETY: dlerror returns a thread-local message if one is available.
            let detail = unsafe {
                let ptr = libc::dlerror();
                if ptr.is_null() {
                    "dlopen failed".to_owned()
                } else {
                    CStr::from_ptr(ptr).to_string_lossy().into_owned()
                }
            };
            return Err(io::Error::other(detail));
        }
        Ok(Self(handle))
    }

    fn address(&self, symbol: &CStr) -> io::Result<*mut c_void> {
        // SAFETY: the library handle and symbol name are both valid for this call.
        let address = unsafe { libc::dlsym(self.0, symbol.as_ptr()) };
        if address.is_null() {
            return Err(io::Error::other(format!(
                "missing {}",
                symbol.to_string_lossy()
            )));
        }
        Ok(address)
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: self owns a successful dlopen handle.
        unsafe { libc::dlclose(self.0) };
    }
}

pub fn serve(path: &Path, referee: u32) -> io::Result<()> {
    // SAFETY: PR_SET_PDEATHSIG sets only the worker's death signal.
    if unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0) } == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: getppid has no arguments and reports the current parent.
    if (unsafe { libc::getppid() }) as u32 != referee {
        return Err(io::Error::other("bot referee already exited"));
    }
    // SAFETY: PR_SET_DUMPABLE changes only this worker's dump policy.
    if unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) } == -1 {
        return Err(io::Error::last_os_error());
    }
    let library = Library::open(path)?;
    let version = library.address(c"arena_bot_abi_version")?;
    let decide = library.address(c"arena_bot_decide")?;
    // SAFETY: the ABI requires these two exported symbols to have these signatures.
    let version: Version = unsafe { std::mem::transmute(version) };
    let decide: Decide = unsafe { std::mem::transmute(decide) };
    let mut channel = File::from(io::stdin().as_fd().try_clone_to_owned()?);
    // SAFETY: version is valid while the library handle is open.
    let version = unsafe { version() };
    channel.write_all(&version.to_le_bytes())?;
    if version != ABI_VERSION {
        return Err(io::Error::other("unsupported bot ABI version"));
    }
    let mut data = Vec::new();
    loop {
        let mut length = [0; 4];
        match channel.read_exact(&mut length) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error),
        }
        let length = u32::from_le_bytes(length) as usize;
        if length > MAX_OBSERVATION {
            return Err(io::Error::other("invalid observation length"));
        }
        data.resize(length, 0);
        channel.read_exact(&mut data)?;
        // SAFETY: data is valid for the duration of this ABI call.
        let action = unsafe { decide(data.as_ptr(), data.len()) };
        channel.write_all(&action.to_le_bytes())?;
    }
}
