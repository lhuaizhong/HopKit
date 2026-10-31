//! Socket operations have a fixed deadline and check cancellation at most every 50 ms.
use crate::{runtime::*, target};
use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};
pub fn connect(
    addr: SocketAddr,
    source: &str,
    c: &Cancel,
    deadline: Instant,
) -> Result<TcpStream, ToolError> {
    use socket2::{Domain, Protocol, Socket, Type};
    cancelled(c)?;
    let s = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;
    if !source.is_empty() {
        let bind = target::literal_socket(source, 0).ok_or_else(|| {
            ToolError::new(
                "INVALID_ARGUMENT",
                "Numeric local source IP required / 源地址须为本机 IP，IPv6 scope 须为数字",
            )
        })?;
        if bind.is_ipv4() != addr.is_ipv4() {
            return Err(ToolError::new(
                "UNSUPPORTED_CAPABILITY",
                "Source/destination family mismatch / 源和目标地址族不一致",
            ));
        }
        s.bind(&bind.into())
            .map_err(|e| ToolError::new("SOURCE_BIND_FAILED", e.to_string()))?;
    }
    s.set_nonblocking(true)?;
    match s.connect(&addr.into()) {
        Ok(()) => {}
        Err(e)
            if matches!(e.raw_os_error(), Some(36 | 115 | 10035 | 10036 | 10037))
                || e.kind() == io::ErrorKind::WouldBlock => {}
        Err(e) => return Err(connect_error(e)),
    }
    let stream: TcpStream = s.into();
    loop {
        cancelled(c)?;
        if let Some(e) = stream.take_error()? {
            return Err(connect_error(e));
        }
        if stream.peer_addr().is_ok() {
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(Duration::from_millis(50)))?;
            stream.set_write_timeout(Some(Duration::from_millis(50)))?;
            return Ok(stream);
        }
        if Instant::now() >= deadline {
            return Err(ToolError::new(
                "TIMEOUT",
                "TCP connection timed out / TCP 建连超时",
            ));
        }
        wait(c, Duration::from_millis(10))?;
    }
}
fn connect_error(e: io::Error) -> ToolError {
    ToolError::new(
        if e.kind() == io::ErrorKind::ConnectionRefused {
            "CONNECTION_REFUSED"
        } else if is_timeout(&e) {
            "TIMEOUT"
        } else {
            "CONNECT_ERROR"
        },
        e.to_string(),
    )
}
pub struct ControlledIo<'a> {
    pub socket: TcpStream,
    pub cancel: &'a Cancel,
    pub deadline: Instant,
}
impl ControlledIo<'_> {
    fn check(&self) -> io::Result<()> {
        if cancelled(self.cancel).is_err() {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Cancelled",
            ));
        }
        if Instant::now() >= self.deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Stage deadline exceeded",
            ));
        }
        Ok(())
    }
}
impl Read for ControlledIo<'_> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        loop {
            self.check()?;
            match self.socket.read(b) {
                Err(e) if is_timeout(&e) || e.kind() == io::ErrorKind::Interrupted => continue,
                result => return result,
            }
        }
    }
}
impl Write for ControlledIo<'_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        loop {
            self.check()?;
            match self.socket.write(b) {
                Err(e) if is_timeout(&e) || e.kind() == io::ErrorKind::Interrupted => continue,
                result => return result,
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.check()?;
        self.socket.flush()
    }
}
pub fn io_error(error: io::Error, c: &Cancel) -> ToolError {
    if let Err(e) = cancelled(c) {
        e
    } else {
        error.into()
    }
}
