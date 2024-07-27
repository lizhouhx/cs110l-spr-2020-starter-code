use libc::option;
use nix::sys::ptrace;
use nix::sys::signal;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::process::Child;
use std::os::unix::process::CommandExt;
use std::process::Command;

use crate::dwarf_data::DwarfData;

pub enum Status {
    /// Indicates inferior stopped. Contains the signal that stopped the process, as well as the
    /// current instruction pointer that it is stopped at.
    Stopped(signal::Signal, usize),

    /// Indicates inferior exited normally. Contains the exit status code.
    Exited(i32),

    /// Indicates the inferior exited due to a signal. Contains the signal that killed the
    /// process.
    Signaled(signal::Signal),
}

/// This function calls ptrace with PTRACE_TRACEME to enable debugging on a process. You should use
/// pre_exec with Command to call this in the child process.
fn child_traceme() -> Result<(), std::io::Error> {
    ptrace::traceme().or(Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "ptrace TRACEME failed",
    )))
}

fn align_addr_to_word(addr: u64) -> u64 {
    addr & (-(size_of::<u64>() as i64) as u64)
}

pub struct Inferior {
    child: Child,
}

impl Inferior {
    /// Attempts to start a new inferior process. Returns Some(Inferior) if successful, or None if
    /// an error is encountered.
    pub fn new(target: &str, args: &Vec<String>, breakpoints: &mut HashMap<u64, u8>) -> Option<Inferior> {
        // TODO: implement me!
        let mut cmd = Command::new(target);
        cmd.args(args);
        unsafe {
            cmd.pre_exec(child_traceme);
        }
        let child = cmd.spawn().ok()?;
        let mut inferior = Inferior{child};
        
        if inferior.wait(None).is_ok() {
            for &it in breakpoints.clone().keys(){
                match inferior.write_byte(it, 0xcc){//0xcc corresponds to the INT ("interrupt") instruction
                    Ok(orig_byte) => {breakpoints.insert(it, orig_byte);},
                    Err(_) => println!("Invalid breakpoint address: {:#x}", it),
                }
            }
            return Some(inferior);
        }
        None
    }

    //continue to run
    pub fn keep(&mut self, breakpoints: &HashMap<u64, u8>) -> Result<Status, nix::Error>{
        let mut regs = ptrace::getregs(self.pid())?;
        let rip = regs.rip;
        if let Some(_) = breakpoints.get(&(rip-1)){
            ptrace::step(self.pid(), None)?;
            match self.wait(None).unwrap(){
                Status::Stopped(_, _) => {
                    //restore 0xcc in the breakpoint location
                    self.write_byte(rip-1, 0xcc)?;
                },
                Status::Exited(exit_code) => return Ok(Status::Exited(exit_code)),
                Status::Signaled(signal) => return Ok(Status::Signaled(signal)),
            }
        }

        ptrace::cont(self.pid(), None)?;
        let result = self.wait(None); 

        if let Some(orig_byte) = breakpoints.get(&(rip-1)){
            //restore the first byte of the instruction we replaced
            self.write_byte(rip, *orig_byte)?;
            //rewind the instruction pointer
            regs.rip = rip - 1;
            ptrace::setregs(self.pid(), regs)?;
        }  
        result
    }

    pub fn kill(&mut self) -> Pid {
        let pid = self.pid();
        let _ = self.child.kill();
        pid
    }

    pub fn print_backtrace(&self, debug_data: &DwarfData) -> Result<(), nix::Error>{
        let regs = ptrace::getregs(self.pid())?;
        let mut rip = regs.rip as usize;//instruction_ptr
        let mut rbp = regs.rbp as usize;//base_ptr
        loop{
            let line = DwarfData::get_line_from_addr(debug_data, rip).unwrap();
            let function = DwarfData::get_function_from_addr(debug_data, rip).unwrap();
            println!("{} ({})",function, line);
            if function == "main"{
                break;
            }
            rip = ptrace::read(self.pid(), (rbp + 8) as ptrace::AddressType)? as usize;//rbq + 8 to get the return address
            rbp = ptrace::read(self.pid(), rbp as ptrace::AddressType)? as usize;
        }     
        Ok(())
    }

    /// Returns the pid of this inferior.
    pub fn pid(&self) -> Pid {
        nix::unistd::Pid::from_raw(self.child.id() as i32)
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

    pub fn write_byte(&mut self, addr: u64, val: u8) -> Result<u8, nix::Error> {
        let aligned_addr = align_addr_to_word(addr);
        let byte_offset = addr - aligned_addr;
        let word = ptrace::read(self.pid(), aligned_addr as ptrace::AddressType)? as u64;
        let orig_byte = (word >> 8 * byte_offset) & 0xff;
        let masked_word = word & !(0xff << 8 * byte_offset);
        let updated_word = masked_word | ((val as u64) << 8 * byte_offset);
        ptrace::write(
            self.pid(),
            aligned_addr as ptrace::AddressType,
            updated_word as *mut std::ffi::c_void,
        )?;
        Ok(orig_byte as u8)
    }
}
