use nix::sys::ptrace;
use nix::sys::signal::{Signal, SIGTRAP};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use nix::Error;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};

pub enum Status {
    /// Indicates inferior stopped. Contains the signal that stopped the process, as well as the
    /// current instruction pointer that it is stopped at.
    Stopped(Signal, usize),

    /// Indicates inferior exited normally. Contains the exit status code.
    Exited(i32),

    /// Indicates the inferior exited due to a signal. Contains the signal that killed the
    /// process.
    Signaled(Signal),
}

/// This function calls ptrace with PTRACE_TRACEME to enable debugging on a process. You should use
/// pre_exec with Command to call this in the child process.
fn child_traceme() -> Result<(), std::io::Error> {
    ptrace::traceme()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "ptrace TRACEME failed"))
}

pub struct Inferior {
    child: Child,
}

///  An inferior is a process that is being traced by the debugger.
impl Inferior {
    /// Attempts to start a new inferior process. Returns Some(Inferior) if successful, or None if
    /// an error is encountered.
    pub fn new(target: &str, args: &Vec<String>) -> Option<Inferior> {
        unsafe {
            Command::new(target)
                .args(args)
                .pre_exec(child_traceme)
                .spawn()
                .map(|child| {
                    let inferior = Inferior { child };

                    if let Status::Stopped(SIGTRAP, _) =
                        inferior.wait(Some(WaitPidFlag::WSTOPPED)).unwrap()
                    {
                        inferior
                    } else {
                        Err(Error::Sys(nix::errno::Errno::EIO)).unwrap()
                    }
                })
                .ok()
        }
    }

    /// resume the inferior from initial SIGTRAP
    pub fn continue_exec(&self) -> Result<Status, Error> {
        ptrace::cont(self.pid(), None)?;
        self.wait(None)
    }

    /// Returns the pid of this inferior.
    pub fn pid(&self) -> Pid {
        Pid::from_raw(self.child.id() as i32)
    }

    /// Calls waitpid on this inferior and returns a Status to indicate the state of the process
    /// after the waitpid call.
    pub fn wait(&self, options: Option<WaitPidFlag>) -> Result<Status, nix::Error> {
        Ok(match waitpid(self.pid(), options)? {
            WaitStatus::Exited(_pid, exit_code) => Status::Exited(exit_code),
            WaitStatus::Signaled(_pid, signal, _core_dumped) => Status::Signaled(signal),
            WaitStatus::Stopped(_pid, signal) => {
                let regs = ptrace::getregs(self.pid())?;
                Status::Stopped(signal, regs.rip as usize)
            }
            other => panic!("waitpid returned unexpected status: {:?}", other),
        })
    }
}
