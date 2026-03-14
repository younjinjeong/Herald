use tokio::net::UnixStream;
use std::os::unix::io::AsRawFd;

use crate::error::{HeraldError, Result};

#[derive(Debug, Clone)]
pub struct PeerCredentials {
    pub pid: u32,
    pub uid: u32,
    pub gid: u32,
}

pub fn verify_peer(stream: &UnixStream) -> Result<PeerCredentials> {
    let fd = stream.as_raw_fd();

    // Safety: fd is valid as long as stream is alive
    let borrowed_fd = unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) };

    let cred = nix::sys::socket::getsockopt(
        &borrowed_fd,
        nix::sys::socket::sockopt::PeerCredentials,
    )
    .map_err(|e| HeraldError::Security(format!("Failed to get peer credentials: {}", e)))?;

    let my_uid = nix::unistd::getuid().as_raw();
    let peer_uid = cred.uid();

    if peer_uid != my_uid {
        return Err(HeraldError::Security(format!(
            "Peer UID {} does not match daemon UID {}",
            peer_uid, my_uid
        )));
    }

    Ok(PeerCredentials {
        pid: cred.pid() as u32,
        uid: cred.uid(),
        gid: cred.gid(),
    })
}
