use crate::debugger_command::DebuggerCommand;
use crate::inferior::{Inferior, Status};
use nix::Error;
use rustyline::error::ReadlineError;
use rustyline::Editor;
use crate::dwarf_data::{DwarfData,Error as DwarfError};

pub struct Debugger {
    target: String,
    history_path: String,
    readline: Editor<()>,
    inferior: Option<Inferior>,
    debug_data:DwarfData
}

impl Debugger {
    /// Initializes the debugger.
    pub fn new(target: &str) -> Debugger {
        // TODO (milestone 3): initialize the DwarfData

        let history_path = format!("{}/.deet_history", std::env::var("HOME").unwrap());
        let mut readline = Editor::<()>::new();
        // Attempt to load history from ~/.deet_history if it exists
        let _ = readline.load_history(&history_path);
        let debug_data = match DwarfData::from_file(target) {
            Ok(val) => val,
            Err(DwarfError::ErrorOpeningFile) => {
                println!("Could not open file {}", target);
                std::process::exit(1);
            }
            Err(DwarfError::DwarfFormatError(err)) => {
                println!("Could not debugging symbols from {}: {:?}", target, err);
                std::process::exit(1);
            }
        };

        Debugger {
            target: target.to_string(),
            history_path,
            readline,
            inferior: None,
            debug_data,
        }
    }

    pub fn run(&mut self) {
        loop {
            let next = match self.get_next_command() {
                DebuggerCommand::Run(args) => {
                    if let Some(inferior) = self.inferior.as_mut() {
                        inferior.kill()
                    } else if let Some(inferior) = Inferior::new(&self.target, &args) {
                        // Create the inferior
                        self.inferior = Some(inferior);
                        // You may use self.inferior.as_mut().unwrap() to get a mutable reference
                        // to the Inferior object
                        self.inferior.as_mut().unwrap().continue_exec()
                    } else {
                        Err(Error::Sys(nix::errno::Errno::EIO))
                    }
                }

                DebuggerCommand::Continue => {
                    if let Some(inferior) = self.inferior.as_mut() {
                        inferior.continue_exec()
                    } else {
                        Err(Error::Sys(nix::errno::Errno::EIO))
                    }
                }

                DebuggerCommand::Quit => {
                    if let Some(inferior) = self.inferior.as_mut() {
                        inferior.kill().expect("Kill failed");
                        return;
                    } else {
                        Err(Error::Sys(nix::errno::Errno::EIO))
                    }
                }
                DebuggerCommand::Backtrace => {
                    if let Some(inferior) = self.inferior.as_mut() {
                        inferior.print_backtrace(&self.debug_data)
                    } else {
                        Err(Error::Sys(nix::errno::Errno::EIO))
                    }
                }
            };

            match next {
                Ok(status) => match status {
                    Status::Exited(_code) => {
                        println!("Child exited (status {})", _code);
                        self.inferior = None;
                    }
                    Status::Signaled(_signal) => {
                        println!("Child signaled (signal {})", _signal);
                        self.inferior = None;
                    }
                    Status::Stopped(_signal, _) => {
                        println!("Child stopped (signal {})", _signal);
                    }
                    Status::None => {}
                },
                Err(_) => {
                    println!("Error running inferior");
                }
            }
        }
    }

    /// This function prompts the user to enter a command, and continues re-prompting until the user
    /// enters a valid command. It uses DebuggerCommand::from_tokens to do the command parsing.
    ///
    /// You don't need to read, understand, or modify this function.
    fn get_next_command(&mut self) -> DebuggerCommand {
        loop {
            // Print prompt and get next line of user input
            match self.readline.readline("(deet) ") {
                Err(ReadlineError::Interrupted) => {
                    // User pressed ctrl+c. We're going to ignore it
                    println!("Type \"quit\" to exit");
                }
                Err(ReadlineError::Eof) => {
                    // User pressed ctrl+d, which is the equivalent of "quit" for our purposes
                    return DebuggerCommand::Quit;
                }
                Err(err) => {
                    panic!("Unexpected I/O error: {:?}", err);
                }
                Ok(line) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    self.readline.add_history_entry(line.as_str());
                    if let Err(err) = self.readline.save_history(&self.history_path) {
                        println!(
                            "Warning: failed to save history file at {}: {}",
                            self.history_path, err
                        );
                    }
                    let tokens: Vec<&str> = line.split_whitespace().collect();
                    if let Some(cmd) = DebuggerCommand::from_tokens(&tokens) {
                        return cmd;
                    } else {
                        println!("Unrecognized command.");
                    }
                }
            }
        }
    }
}
