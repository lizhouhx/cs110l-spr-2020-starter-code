use std::collections::HashMap;
use crate::debugger_command::DebuggerCommand;
use crate::inferior::{Inferior, Status};
use crate::dwarf_data::{self, DwarfData, Error as DwarfError};
use rustyline::error::ReadlineError;
use rustyline::Editor;

pub struct Debugger {
    target: String,
    history_path: String,
    readline: Editor<()>,
    inferior: Option<Inferior>,
    debug_data: DwarfData,
    breakpoints: HashMap<u64, u8>,
}

impl Debugger {
    /// Initializes the debugger.
    pub fn new(target: &str) -> Debugger {
        // TODO (milestone 3): initialize the DwarfData
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
        debug_data.print();

        let history_path = format!("{}/.deet_history", std::env::var("HOME").unwrap());
        let mut readline = Editor::<()>::new();
        // Attempt to load history from ~/.deet_history if it exists
        let _ = readline.load_history(&history_path);

        Debugger {
            target: target.to_string(),
            history_path,
            readline,
            inferior: None,
            debug_data,
            breakpoints: HashMap::new(),
        }
    }

    pub fn run(&mut self) {
        loop {
            match self.get_next_command() {
                DebuggerCommand::Run(args) => {
                    if self.inferior.is_some(){
                        let pid = self.inferior.as_mut().unwrap().kill();
                        println!("Killing running inferior (pid {})", pid);
                    }
                    if let Some(inferior) = Inferior::new(&self.target, &args, &mut self.breakpoints) {
                        // Create the inferior
                        self.inferior = Some(inferior);
                        // TODO (milestone 1): make the inferior run                 
                        // You may use self.inferior.as_mut().unwrap() to get a mutable reference
                        // to the Inferior object
                        let status = self.inferior.as_mut().unwrap().keep(&self.breakpoints).expect("Something Occurs during 'Run' !");
                        match status{
                            Status::Exited(exit) => println!("Child exited (status {})", exit),
                            Status::Signaled(signal) => println!("Child output signal {} ",signal),
                            Status::Stopped(signal, rip) => {
                                println!("Child stopped (signal {})", signal);
                                let line = DwarfData::get_line_from_addr(&self.debug_data, rip).unwrap();
                                let function = DwarfData::get_function_from_addr(&self.debug_data, rip).unwrap();
                                println!("Stopped at '{}' {}",function, line);
                            }
                        }
                        
                    } else {
                        println!("Error starting subprocess");
                    }
                }
                DebuggerCommand::Quit => {
                    if self.inferior.is_some(){
                        let pid = self.inferior.as_mut().unwrap().kill();
                        println!("Killing running inferior (pid {})", pid);
                    }
                    return;
                }
                DebuggerCommand::Continue => {
                    if self.inferior.is_some(){
                        let status = self.inferior.as_mut().unwrap().keep(&self.breakpoints).expect("Something Occurs during 'Continue' !");
                        match status{
                            Status::Exited(exit) => println!("Child exited (status {})", exit),
                            Status::Signaled(signal) => println!("Child output signal {} ",signal),
                            Status::Stopped(signal, rip) => {
                                println!("Child stopped (signal {})", signal);
                                let line = DwarfData::get_line_from_addr(&self.debug_data, rip).unwrap();
                                let function = DwarfData::get_function_from_addr(&self.debug_data, rip).unwrap();
                                println!("Stopped at '{}' {}",function, line);
                            }
                        }
                    }
                    else{
                        println!("No running inferior exit!");
                    }
                }
                DebuggerCommand::Backtrace => {
                    if self.inferior.is_some(){
                        let _ = self.inferior.as_mut().unwrap().print_backtrace(&self.debug_data);
                    }
                    else{
                        println!("No running inferior exit!");
                    }
                }
                DebuggerCommand::Break(args) =>{
                    let address;
                    let addr = args.as_str();
                    if addr.starts_with("*"){                      
                        address = parse_address(&addr[1..]);
                    }
                    else if let Some(line) = usize::from_str_radix(&args, 10).ok(){                     
                        if let Some(_address) = self.debug_data.get_addr_for_line(None,line){
                            address = Some(_address as u64);
                        }
                        else{
                            println!("Invalid position to set breakpoint!");
                            continue;
                        }           
                    }
                    else if let Some(_address) = self.debug_data.get_addr_for_function(None, addr){
                        address = Some(_address as u64);
                    }
                    else{
                        println!("Invalid position to set breakpoint!");
                        continue;
                    }
                   
                    if self.inferior.is_some(){
                        if let Some(instruction) = self.inferior.as_mut().unwrap().write_byte(address.unwrap(), 0xcc).ok(){
                            self.breakpoints.insert(address.unwrap(), instruction);
                            println!("Set breakpoint {} at {:#x}",self.breakpoints.len(), address.unwrap());
                        }
                    }
                    else{
                        self.breakpoints.insert(address.unwrap(), 0);
                        println!("Set breakpoint {} at {:#x}",self.breakpoints.len(), address.unwrap());
                    }                              
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
                    if line.trim().len() == 0 {
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

fn parse_address(addr: &str) -> Option<u64> {
    let addr_without_0x = if addr.to_lowercase().starts_with("0x") {
        &addr[2..]
    } else {
        &addr
    };
    u64::from_str_radix(addr_without_0x, 16).ok()
}