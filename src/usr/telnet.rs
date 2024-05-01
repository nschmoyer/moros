use crate::api::console::Style;
use crate::api::process::ExitCode;
use crate::api::syscall;
use crate::sys::console;
use crate::sys::fs::OpenFlag;
use crate::sys::net::SocketStatus;
use crate::api::fs::IO;
use crate::api::io;
use crate::usr;

use alloc::string::String;
use alloc::vec;
use bit_field::BitField;
use core::str::{self, FromStr};
use smoltcp::wire::IpAddress;
use crate::sys::console::{disable_echo, enable_echo};

#[derive(Debug)]
struct Connection {
    pub host: String,
    pub port: u16,
}

impl Connection {
    pub fn parse(url: &str) -> (&str, u16) {
        let (host, port) = match url.find(':') {
            Some(i) => url.split_at(i),
            None => (url, ":23"),
        };
        let port = &port[1..];

        (host.into(), port.parse().unwrap_or(23))
    }
}

pub fn main(args: &[&str]) -> Result<(), ExitCode> {
    let csi_verbose = Style::color("LightBlue");
    let csi_reset = Style::reset();

    // Parse command line options
    let mut is_verbose = false;
    let mut host = "";
    let mut timeout = 5.0;
    let mut i = 1;
    let n = args.len();
    while i < n {
        match args[i] {
            "-h" | "--help" => {
                return help();
            }
            "-v" | "--verbose" => {
                is_verbose = true;
            }
            "-t" | "--timeout" => {
                if i + 1 < n {
                    timeout = args[i + 1].parse().unwrap_or(timeout);
                    i += 1;
                } else {
                    error!("Missing timeout seconds");
                    return Err(ExitCode::UsageError);
                }
            }
            _ => {
                if args[i].starts_with('-') {
                    error!("Invalid option '{}'", args[i]);
                    return Err(ExitCode::UsageError);
                } else if host.is_empty() {
                    host = args[i];
                } else {
                    error!("Too many arguments");
                    return Err(ExitCode::UsageError);
                }
            }
        }
        i += 1;
    }

    if host.is_empty() {
        error!("Missing host");
        return Err(ExitCode::UsageError);
    }

    let (host, port) = Connection::parse(&host);

    let addr = if host.ends_with(char::is_numeric) {
        match IpAddress::from_str(&host) {
            Ok(ip_addr) => ip_addr,
            Err(_) => {
                error!("Invalid address format");
                return Err(ExitCode::UsageError);
            }
        }
    } else {
        match usr::host::resolve(&host) {
            Ok(ip_addr) => ip_addr,
            Err(e) => {
                error!("Could not resolve host: {:?}", e);
                return Err(ExitCode::Failure);
            }
        }
    };

    let socket_path = "/dev/net/tcp";
    let buf_len = if let Some(info) = syscall::info(socket_path) {
        info.size() as usize
    } else {
        error!("Could not open '{}'", socket_path);
        return Err(ExitCode::Failure);
    };

    let mut connected = false;
    let stdin = 0;
    let stdout = 1;
    let flags = OpenFlag::Device as usize;
    if let Some(handle) = syscall::open(socket_path, flags) {

        if syscall::connect(handle, addr, port).is_ok() {
            connected = true;
        } else {
            error!("Could not connect to {}:{}", addr, port);
            syscall::close(handle);
            return Err(ExitCode::Failure);
        }
        if is_verbose {
            debug!("Connected to {}:{}", addr, port);
        }

        loop {
            if console::end_of_text() || console::end_of_transmission() {
                println!();
                break;
            }

            let list = vec![(stdin, IO::Read), (handle, IO::Read)];
            if let Some((h, _)) = syscall::poll(&list) {
                if h == stdin {
                    let line = io::stdin().read_line().replace("\n", "\r\n");
                    syscall::write(handle, line.as_bytes());
                } else {
                    let mut data = vec![0; buf_len];
                    if let Some(bytes) = syscall::read(handle, &mut data) {
                        data.resize(bytes, 0);

                        let mut i = 0;
                        while i < data.len() {
                            // Check and handle IAC sequences
                            if handle_iac(&data, &mut i, handle) {
                                i += 1;
                                continue; // Skip the rest of the loop since we've handled an IAC command
                            }

                            // Output the data if not part of a Telnet command
                            syscall::write(stdout, &[data[i]]);
                            i += 1;
                        }
                    }
                }
            } else {
                syscall::sleep(0.01);
                if connected {
                    let mut data = vec![0; 1]; // 1 byte status read
                    match syscall::read(handle, &mut data) {
                        Some(1) if is_closed(data[0]) => break,
                        _ => continue,
                    }
                }
            }
        }
        syscall::close(handle);
        Ok(())
    } else {
        Err(ExitCode::Failure)
    }
}

fn handle_iac(data: &[u8], i: &mut usize, handle: usize) -> bool {
    if data[*i] == BYTE_IAC && *i + 2 < data.len() {
        match (data[*i + 1], data[*i + 2]) {
            (BYTE_DO, BYTE_TERMINAL_TYPE) => {
                let response = [
                    BYTE_IAC,
                    BYTE_SB,
                    BYTE_TERMINAL_TYPE,
                    0,
                    b'X', b'T', b'E', b'R', b'M', b'-', b'2', b'5', b'6', b'C', b'O', b'L', b'O', b'R',
                    BYTE_IAC,
                    BYTE_SE,
                ];
                syscall::write(handle, &response);
                *i += 2;
            },
            (BYTE_WILL, BYTE_SUPPRESS_LOCAL_ECHO) => {
                disable_echo();
                let response = [BYTE_IAC, BYTE_DO, BYTE_SUPPRESS_LOCAL_ECHO];
                syscall::write(handle, &response);
                *i += 2;
            },
            (BYTE_WONT, BYTE_SUPPRESS_LOCAL_ECHO) => {
                enable_echo();
                let response = [BYTE_IAC, BYTE_DONT, BYTE_SUPPRESS_LOCAL_ECHO];
                syscall::write(handle, &response);
                *i += 2;
            },
            _ => return false, // Not a sequence we handle, move past the IAC byte
        }
        true // It was an IAC sequence
    } else {
        false // Not an IAC byte
    }
}

fn is_closed(status: u8) -> bool {
    !status.get_bit(SocketStatus::MayRecv as usize)
}

fn help() -> Result<(), ExitCode> {
    let csi_option = Style::color("LightCyan");
    let csi_title = Style::color("Yellow");
    let csi_reset = Style::reset();
    println!(
        "{}Usage:{} telnet {}<options> host:port{1}",
        csi_title, csi_reset, csi_option
    );
    println!();
    println!("{}Options:{}", csi_title, csi_reset);
    println!(
        "  {0}-v{1}, {0}--verbose{1}              Increase verbosity",
        csi_option, csi_reset
    );
    println!(
        "  {0}-t{1}, {0}--timeout <seconds>{1}    Request timeout",
        csi_option, csi_reset
    );
    Ok(())
}

/// Telnet negotiation commands
pub const BYTE_IAC: u8 = 255; // interpret as command:
pub const BYTE_DONT: u8 = 254; // you are not to use option
pub const BYTE_DO: u8 = 253; // please, you use option
pub const BYTE_WONT: u8 = 252; // I won't use option
pub const BYTE_WILL: u8 = 251; // I will use option
pub const BYTE_SB: u8 = 250; // interpret as subnegotiation
pub const BYTE_SE: u8 = 240; // end sub negotiation
pub const BYTE_TERMINAL_TYPE: u8 = 24; // terminal type
pub const BYTE_SUPPRESS_LOCAL_ECHO: u8 = 1; // suppress local echo
