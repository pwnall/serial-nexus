use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::PathBuf;
use nix::pty::{self, OpenptyResult};
use nix::sys::termios::{self, SetArg};
use nix::unistd;
use std::io::{Read, Write};
use std::fs::File;

/// Connected to a pseudo-TTY file.
#[derive(Debug)]
pub struct SerialConnection {
    tty_control_file: File,
}

impl SerialConnection {
    /// Creates a new SerialConnection from an OwnedFd.
    pub fn new(master: OwnedFd) -> Self {
        Self { tty_control_file: File::from(master) }
    }
}

impl AsFd for SerialConnection {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.tty_control_file.as_fd()
    }
}

impl Read for SerialConnection {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.tty_control_file.read(buf)
    }
}

impl Write for SerialConnection {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.tty_control_file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.tty_control_file.flush()
    }
}

pub struct CreateSerialFileResult {
    /// Can be passed to applications that expect a TTY device.
    pub serial_file: PathBuf,

    /// Simulates the other end of the serial file.
    pub connection: SerialConnection,
}

/// Creates a pseudo-TTY file.
pub fn create_serial_file() -> Result<CreateSerialFileResult, Box<dyn std::error::Error>> {
    let OpenptyResult { master, slave } = pty::openpty(None, None)?;
    let path = unistd::ttyname(&slave)?;

    // Set raw mode on the slave PTY so it behaves like a raw serial port
    let mut termios = termios::tcgetattr(&slave)?;
    termios::cfmakeraw(&mut termios);
    termios::tcsetattr(&slave, SetArg::TCSANOW, &termios)?;

    Ok(CreateSerialFileResult {
        serial_file: path,
        connection: SerialConnection::new(master),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::fs::File;

    #[test]
    fn test_serial_path_exists() {
        let result = create_serial_file().expect("Failed to create serial file");
        assert!(result.serial_file.exists());
    }

    #[test]
    fn test_host_receives_device_log() {
        let mut result = create_serial_file().expect("Failed to create serial file");

        let mut host_file = File::options()
            .read(true)
            .write(true)
            .open(&result.serial_file)
            .expect("Failed to open serial file");

        let device_message = b"device log line";
        result.connection.write_all(device_message).expect("Failed to write to controller");
        result.connection.flush().expect("Failed to flush controller");

        let mut host_buffer = [0u8; 15];
        host_file.read_exact(&mut host_buffer).expect("Failed to read from serial file");
        assert_eq!(&host_buffer, device_message);
    }

    #[test]
    fn test_device_receives_host_command() {
        let mut result = create_serial_file().expect("Failed to create serial file");

        let mut host_file = File::options()
            .read(true)
            .write(true)
            .open(&result.serial_file)
            .expect("Failed to open serial file");

        let host_message = b"command from host";
        host_file.write_all(host_message).expect("Failed to write to serial file");
        host_file.flush().expect("Failed to flush serial file");

        let mut device_buffer = [0u8; 17];
        result.connection.read_exact(&mut device_buffer).expect("Failed to read from controller");
        assert_eq!(&device_buffer, host_message);
    }

    #[test]
    fn test_command_response() {
        let mut result = create_serial_file().expect("Failed to create serial file");

        let mut host_file = File::options()
            .read(true)
            .write(true)
            .open(&result.serial_file)
            .expect("Failed to open serial file");

        // Test communication: TTY file -> Control
        let host_message = b"command for device";
        host_file.write_all(host_message).expect("Failed to write to serial file");
        host_file.flush().expect("Failed to flush serial file");

        let mut device_buffer = [0u8; 18];
        result.connection.read_exact(&mut device_buffer).expect("Failed to read from controller");
        assert_eq!(&device_buffer, host_message);

        let host_message = b"command from host";
        host_file.write_all(host_message).expect("Failed to write to serial file");
        host_file.flush().expect("Failed to flush serial file");

        let mut device_buffer = [0u8; 17];
        result.connection.read_exact(&mut device_buffer).expect("Failed to read from controller");
        assert_eq!(&device_buffer, host_message);

        let device_message = b"device response";
        result.connection.write_all(device_message).expect("Failed to write to controller");
        result.connection.flush().expect("Failed to flush controller");

        let mut host_buffer = [0u8; 15];
        host_file.read_exact(&mut host_buffer).expect("Failed to read from serial file");
        assert_eq!(&host_buffer, device_message);
    }
}
