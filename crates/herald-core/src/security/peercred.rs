use crate::error::{HeraldError, Result};

#[derive(Debug, Clone)]
pub struct PeerCredentials {
    pub pid: u32,
    pub uid: u32,
    pub gid: u32,
}

#[cfg(target_os = "linux")]
pub fn verify_peer(stream: &tokio::net::UnixStream) -> Result<PeerCredentials> {
    use std::os::unix::io::AsRawFd;

    let fd = stream.as_raw_fd();
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

#[cfg(target_os = "macos")]
pub fn verify_peer(stream: &tokio::net::UnixStream) -> Result<PeerCredentials> {
    use std::os::unix::io::AsRawFd;

    let fd = stream.as_raw_fd();
    let mut uid: libc::uid_t = 0;
    let mut gid: libc::gid_t = 0;
    let ret = unsafe { libc::getpeereid(fd, &mut uid, &mut gid) };

    if ret != 0 {
        return Err(HeraldError::Security(
            "Failed to get peer credentials via getpeereid".to_string(),
        ));
    }

    let my_uid = unsafe { libc::getuid() };
    if uid != my_uid {
        return Err(HeraldError::Security(format!(
            "Peer UID {} does not match daemon UID {}",
            uid, my_uid
        )));
    }

    Ok(PeerCredentials {
        pid: 0,
        uid,
        gid,
    })
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn verify_peer(_stream: &tokio::net::UnixStream) -> Result<PeerCredentials> {
    tracing::warn!("Peer credential verification not supported on this platform");
    Ok(PeerCredentials { pid: 0, uid: 0, gid: 0 })
}
