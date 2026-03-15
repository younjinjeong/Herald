#[cfg(target_os = "linux")]
pub fn is_process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{}/stat", pid)).exists()
}

#[cfg(not(target_os = "linux"))]
pub fn is_process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}
