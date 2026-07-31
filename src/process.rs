use std::process;

const CODE_SUCCESS: i32 = 0;
const CODE_ERROR: i32 = 2;

#[derive(Debug, Clone, Copy)]
#[repr(i32)]
pub enum ExitStatus {
    Success,
    Error,
    BrokenPipe,
    Jq(process::ExitStatus),
}

#[cfg(not(unix))]
pub fn exit(status: ExitStatus) -> ! {
    process::exit(status.code().unwrap_or(CODE_ERROR))
}

#[cfg(unix)]
pub fn exit(status: ExitStatus) -> ! {
    use std::os::unix::process::ExitStatusExt;

    let code = match status {
        ExitStatus::Success => CODE_SUCCESS,
        ExitStatus::Error => CODE_ERROR,
        ExitStatus::BrokenPipe => exit_with_signal(libc::SIGPIPE),
        ExitStatus::Jq(status) => {
            if let Some(code) = status.code() {
                code
            } else if let Some(signum) = status.signal() {
                exit_with_signal(signum)
            } else {
                CODE_ERROR
            }
        }
    };
    process::exit(code);
}

#[cfg(unix)]
fn exit_with_signal(signum: libc::c_int) -> ! {
    if unsafe { libc::signal(signum, libc::SIG_DFL) } == libc::SIG_ERR {
        std::process::abort();
    }

    if unsafe { libc::raise(signum) } != 0 {
        std::process::abort();
    }

    // Only reachable if signal was blocked or otherwise not delivered.
    std::process::abort()
}

impl ExitStatus {
    #[cfg(not(unix))]
    fn code(&self) -> Option<i32> {
        match self {
            ExitStatus::Success => Some(CODE_SUCCESS),
            ExitStatus::Error => Some(CODE_ERROR),
            ExitStatus::BrokenPipe => None,
            ExitStatus::Jq(status) => status.code(),
        }
    }

    /// Return `self` if it is an error code, otherwise return `other`.
    pub fn or(self, other: ExitStatus) -> ExitStatus {
        match self {
            ExitStatus::Success => other,
            _ => self,
        }
    }
}
