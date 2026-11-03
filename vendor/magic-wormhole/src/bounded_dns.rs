//! HopKit: at most four system DNS workers, five second asynchronous deadline.
use std::{net::{SocketAddr,ToSocketAddrs},sync::atomic::{AtomicUsize,Ordering}};
pub(crate) async fn resolve(host:String,port:u16)->std::io::Result<Vec<SocketAddr>> {
 if let Ok(ip)=host.parse::<std::net::IpAddr>(){return Ok(vec![SocketAddr::new(ip,port)]);}
 static ACTIVE:AtomicUsize=AtomicUsize::new(0);
 ACTIVE.fetch_update(Ordering::SeqCst,Ordering::SeqCst,|n|(n<4).then_some(n+1)).map_err(|_|std::io::Error::other("DNS resolver busy"))?;
 let(tx,rx)=futures::channel::oneshot::channel();
 std::thread::spawn(move||{let result=(host.as_str(),port).to_socket_addrs().map(|a|a.collect());ACTIVE.fetch_sub(1,Ordering::SeqCst);let _=tx.send(result);});
 crate::util::timeout(std::time::Duration::from_secs(5),rx).await.map_err(|_|std::io::Error::new(std::io::ErrorKind::TimedOut,"DNS timeout"))?.map_err(|_|std::io::Error::other("DNS resolver stopped"))?
}
