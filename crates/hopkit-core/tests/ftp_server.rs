use hopkit_core::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

#[test]
fn cancellation_cleans_partial_uploads_and_closes_idle_and_passive_connections() {
    let root = std::env::temp_dir().join(format!("hopkit-ftp-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let request = Request {
        tool: "ftp".into(),
        action: "serve".into(),
        listen: "127.0.0.1".into(),
        port,
        path: root.to_string_lossy().into(),
        writable: true,
        count: 0,
        ..Request::default()
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        run(&request, &worker_cancel, &|v| {
            let _ = tx.send(v);
        })
    });
    // A guard cancels the scoped server even if a client assertion panics.
    struct Guard(Cancel);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }
    let _guard = Guard(cancel.clone());
    let ready = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let addr = ready["listen"].as_str().unwrap();
    fn connect(addr: &str) -> BufReader<TcpStream> {
        let stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut c = BufReader::new(stream);
        let mut line = String::new();
        c.read_line(&mut line).unwrap();
        assert!(line.starts_with("220"));
        c
    }
    fn command(c: &mut BufReader<TcpStream>, cmd: &str, code: &str) -> String {
        writeln!(c.get_mut(), "{cmd}\r").unwrap();
        let mut line = String::new();
        c.read_line(&mut line).unwrap();
        assert!(line.starts_with(code), "{line}");
        line
    }
    let mut control = connect(addr);
    command(&mut control, "USER anonymous", "331");
    command(&mut control, "PASS guest", "230");
    command(&mut control, "TYPE I", "200");
    let epsv = command(&mut control, "EPSV", "229");
    let data_port: u16 = epsv
        .split("|||")
        .nth(1)
        .unwrap()
        .split('|')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let mut data = TcpStream::connect(("127.0.0.1", data_port)).unwrap();
    command(&mut control, "STOR partial.bin", "150");
    data.write_all(b"unfinished upload").unwrap();
    let mut idle = connect(addr);
    command(&mut idle, "USER anonymous", "331");
    command(&mut idle, "PASS guest", "230");
    let passive = command(&mut idle, "EPSV", "229");
    let idle_port: u16 = passive
        .split("|||")
        .nth(1)
        .unwrap()
        .split('|')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let began = Instant::now();
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(worker.join().unwrap().unwrap_err().code, "CANCELLED");
    assert!(began.elapsed() < Duration::from_secs(1));
    assert!(!root.join("partial.bin").exists());
    assert!(
        !std::fs::read_dir(&root).unwrap().any(|f| f
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".part"))
    );
    assert!(TcpStream::connect(("127.0.0.1", idle_port)).is_err());
    let mut byte = [0];
    assert!(matches!(control.read(&mut byte), Ok(0) | Err(_)));
    // An immediately restarted service must be able to reuse the control port.
    let retry = Request {
        tool: "ftp".into(),
        action: "serve".into(),
        listen: "127.0.0.1".into(),
        port,
        path: root.to_string_lossy().into(),
        duration_secs: 1,
        ..Request::default()
    };
    assert!(run(&retry, &Arc::new(AtomicBool::new(false)), &|_| {}).is_ok());
    std::fs::remove_dir_all(root).unwrap();
}
