use nix::sys::ptrace;
use nix::sys::signal::{Signal, SIGTRAP};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use nix::Error;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};
use crate::dwarf_data::DwarfData;

pub enum Status {
    /// Indicates inferior stopped. Contains the signal that stopped the process, as well as the
    /// current instruction pointer that it is stopped at.
    Stopped(Signal, usize),

    /// Indicates inferior exited normally. Contains the exit status code.
    Exited(i32),

    /// Indicates the inferior exited due to a signal. Contains the signal that killed the
    /// process.
    Signaled(Signal),

    None,
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
    pub fn continue_exec(&mut self) -> Result<Status, Error> {
        if !self.check_running() {
            return Ok(Status::None);
        }
        ptrace::cont(self.pid(), None)?;
        self.wait(None)
    }

    pub fn kill(&mut self) -> Result<Status, Error> {
        if !self.check_running() {
            return Ok(Status::None);
        }

        self.child.kill().unwrap();
        println!("Killing running inferior (pid {})", self.pid());
        self.wait(None)
    }

    fn check_running(&mut self) -> bool {
        if let Ok(running) = self.running() {
            if !running {
                println!("No running inferior");
                return false;
            }
        }
        true
    }

    pub fn running(&mut self) -> Result<bool, Error> {
        Ok(match self.child.try_wait() {
            Ok(Some(_)) => false,
            Ok(None) => true,
            Err(e) => panic!("unexpected running status: {:?}", e),
        })
    }

    pub fn print_backtrace(&mut self,debug_data:&DwarfData) -> Result<Status, nix::Error> {
        if !self.check_running() {
            return Ok(Status::None);
        }
        let regs = ptrace::getregs(self.pid()).expect("register read failed");
        let mut instruction_ptr =regs.rip as usize;
        let mut base_ptr =regs.rbp as usize;

        loop {
            let line=debug_data.get_line_from_addr(instruction_ptr).expect("get_line failed");
            let function=debug_data.get_function_from_addr(instruction_ptr).expect("get_function failed");
            println!("{} ({}:{})",function,line.file,line.number);
            if function=="main" {
                break
            }
            instruction_ptr=ptrace::read(self.pid(), (base_ptr+8) as ptrace::AddressType)? as usize;
            base_ptr=ptrace::read(self.pid(),base_ptr as ptrace::AddressType)? as usize;
        }
        Ok(Status::None)
    }

    pub fn print_stopped_location(&mut self,debug_data:&DwarfData)->Result<Status,nix::Error>{
        let regs = ptrace::getregs(self.pid()).expect("register read failed");
        let line=debug_data.get_line_from_addr(regs.rip as usize).expect("get line failed");
        println!("Stopped at {}:{}",line.file,line.number);
        Ok(Status::None)
    }

    /// Returns the pid of this inferior.
    pub fn pid(&self) -> Pid {
        Pid::from_raw(self.child.id() as i32)
    }

    /// Calls waitpid on this inferior and returns a Status to indicate the state of the process
    /// after the waitpid call.
    pub fn wait(&self, options: Option<WaitPidFlag>) -> Result<Status, Error> {
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
