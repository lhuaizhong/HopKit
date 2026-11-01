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
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(ToolError::new(
                "TIMEOUT",
                "TCP connection timed out / TCP 建连超时",
            ));
        }
        if let Some(e) = stream.take_error()? {
            return Err(connect_error(e));
        }
        if connection_ready(&stream, remaining.min(Duration::from_millis(50)))
            .map_err(connect_error)?
        {
            cancelled(c)?;
            if let Some(e) = stream.take_error()? {
                return Err(connect_error(e));
            }
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(Duration::from_millis(50)))?;
            stream.set_write_timeout(Some(Duration::from_millis(50)))?;
            return Ok(stream);
        }
        #[cfg(not(windows))]
        wait(c, Duration::from_millis(10))?;
    }
}

#[cfg(windows)]
fn connection_ready(stream: &TcpStream, timeout: Duration) -> io::Result<bool> {
    use std::os::windows::io::AsRawSocket;
    use windows_sys::Win32::Networking::WinSock::{
        FD_SET, SOCKET_ERROR, TIMEVAL, WSAGetLastError, select,
    };
    // Winsock can expose the destination through getpeername while connect is
    // still pending. Only write readiness confirms a successful connection;
    // exception readiness reports a failed connection, not a TLS failure.
    let mut writable = FD_SET {
        fd_count: 1,
        fd_array: [0; 64],
    };
    writable.fd_array[0] = stream.as_raw_socket() as _;
    let mut errors = writable;
    let timeout = TIMEVAL {
        tv_sec: 0,
        tv_usec: timeout.as_micros() as i32,
    };
    let result = unsafe {
        select(
            0,
            std::ptr::null_mut(),
            &mut writable,
            &mut errors,
            &timeout,
        )
    };
    if result == SOCKET_ERROR {
        let error = io::Error::from_raw_os_error(unsafe { WSAGetLastError() });
        return if error.kind() == io::ErrorKind::Interrupted {
            Ok(false)
        } else {
            Err(error)
        };
    }
    if errors.fd_count != 0 {
        return Err(stream.take_error()?.unwrap_or_else(|| {
            io::Error::new(io::ErrorKind::ConnectionAborted, "TCP connection failed")
        }));
    }
    Ok(writable.fd_count != 0)
}

#[cfg(not(windows))]
fn connection_ready(stream: &TcpStream, _: Duration) -> io::Result<bool> {
    Ok(stream.peer_addr().is_ok())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        net::TcpListener,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
    };

    fn cancel() -> Cancel {
        Arc::new(AtomicBool::new(false))
    }

    #[test]
    fn connected_stream_is_immediately_usable_with_both_address_families() {
        for host in ["127.0.0.1", "::1"] {
            for source in ["", host] {
                let listener = TcpListener::bind((host, 0)).unwrap();
                let addr = listener.local_addr().unwrap();
                let peer = thread::spawn(move || {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut bytes = [0; 4];
                    stream.read_exact(&mut bytes).unwrap();
                    assert_eq!(&bytes, b"test");
                    stream.write_all(b"okay").unwrap();
                });
                let mut stream = connect(
                    addr,
                    source,
                    &cancel(),
                    Instant::now() + Duration::from_secs(2),
                )
                .unwrap();
                assert!(!stream.local_addr().unwrap().ip().is_unspecified());
                assert_eq!(stream.peer_addr().unwrap(), addr);
                stream.write_all(b"test").unwrap();
                let mut bytes = [0; 4];
                stream.read_exact(&mut bytes).unwrap();
                assert_eq!(&bytes, b"okay");
                peer.join().unwrap();
            }
        }
    }

    #[test]
    fn refused_connection_is_reported_before_protocol_io() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let error =
            connect(addr, "", &cancel(), Instant::now() + Duration::from_secs(5)).unwrap_err();
        assert_eq!(error.code, "CONNECTION_REFUSED");
    }

    #[test]
    fn cancellation_and_expired_deadline_prevent_success() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let c = cancel();
        c.store(true, Ordering::Relaxed);
        assert_eq!(
            connect(addr, "", &c, Instant::now() + Duration::from_secs(1))
                .unwrap_err()
                .code,
            "CANCELLED"
        );
        assert_eq!(
            connect(addr, "", &cancel(), Instant::now())
                .unwrap_err()
                .code,
            "TIMEOUT"
        );
    }

    #[cfg(windows)]
    #[test]
    fn pending_connection_does_not_enter_protocol_io_and_remains_cancellable() {
        use socket2::{Domain, Protocol, Socket, Type};
        let listener = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).unwrap();
        listener
            .bind(&"127.0.0.1:0".parse::<SocketAddr>().unwrap().into())
            .unwrap();
        listener.listen(1).unwrap();
        let addr = listener.local_addr().unwrap().as_socket().unwrap();
        // Fill the unaccepted backlog so the next SYN cannot complete. This
        // reproduces Windows returning a peer address for a pending socket.
        let mut pending = Vec::new();
        let mut saturated = false;
        for _ in 0..64 {
            let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).unwrap();
            let result = socket.connect_timeout(&addr.into(), Duration::from_millis(100));
            pending.push(socket);
            if let Err(e) = result {
                assert!(is_timeout(&e), "{e}");
                saturated = true;
                break;
            }
        }
        assert!(saturated, "Could not saturate loopback backlog");
        let began = Instant::now();
        assert_eq!(
            connect(addr, "", &cancel(), began + Duration::from_millis(150))
                .unwrap_err()
                .code,
            "TIMEOUT"
        );
        assert!(began.elapsed() < Duration::from_secs(1));
        let c = cancel();
        let worker_cancel = c.clone();
        let worker = thread::spawn(move || {
            connect(
                addr,
                "",
                &worker_cancel,
                Instant::now() + Duration::from_secs(10),
            )
        });
        thread::sleep(Duration::from_millis(100));
        let began = Instant::now();
        c.store(true, Ordering::Relaxed);
        assert_eq!(worker.join().unwrap().unwrap_err().code, "CANCELLED");
        assert!(began.elapsed() < Duration::from_secs(1));
    }
}
