use anyhow::{Context, Result, bail};
use std::path::PathBuf;

pub const THETA_SOCKET_PATH_ENV: &str = "THETA_SOCKET_PATH";

pub fn theta_socket_path() -> Result<PathBuf> {
    if let Ok(explicit) = std::env::var(THETA_SOCKET_PATH_ENV) {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    let home =
        std::env::var("HOME").context("HOME is not set and THETA_SOCKET_PATH was not provided")?;
    if home.trim().is_empty() {
        bail!("HOME is empty and THETA_SOCKET_PATH was not provided");
    }

    Ok(PathBuf::from(home)
        .join(".theta")
        .join("run")
        .join("theta.sock"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn resolves_socket_path_from_home_by_default() {
        let _guard = env_lock().lock().expect("lock poisoned");
        unsafe {
            std::env::remove_var(THETA_SOCKET_PATH_ENV);
            std::env::set_var("HOME", "/tmp/theta-home");
        }

        let path = theta_socket_path().expect("socket path");
        assert_eq!(path, PathBuf::from("/tmp/theta-home/.theta/run/theta.sock"));
    }

    #[test]
    fn resolves_socket_path_from_env_override() {
        let _guard = env_lock().lock().expect("lock poisoned");
        unsafe {
            std::env::set_var(THETA_SOCKET_PATH_ENV, "/var/run/custom-theta.sock");
            std::env::remove_var("HOME");
        }

        let path = theta_socket_path().expect("socket path");
        assert_eq!(path, PathBuf::from("/var/run/custom-theta.sock"));
    }

    #[test]
    fn errors_when_home_is_missing_and_no_override_is_set() {
        let _guard = env_lock().lock().expect("lock poisoned");
        unsafe {
            std::env::remove_var(THETA_SOCKET_PATH_ENV);
            std::env::remove_var("HOME");
        }

        let err = theta_socket_path().expect_err("missing home should fail");
        assert!(err.to_string().contains("HOME"));
    }
}
