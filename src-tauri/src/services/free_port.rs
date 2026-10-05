//! "Next free port" suggestions for port-conflict help.

use crate::error::{AppError, AppResult};
use std::net::{Ipv4Addr, Ipv6Addr, TcpListener};

const SEARCH_SPAN: u32 = 200;
const MIN_PORT: u16 = 1024;

/// First port in `[max(preferred, 1024), +200]` for which `is_taken` is false.
pub fn first_free(preferred: u16, is_taken: impl Fn(u16) -> bool) -> AppResult<u16> {
    let start = u32::from(preferred.max(MIN_PORT));
    (start..=(start + SEARCH_SPAN).min(u32::from(u16::MAX)))
        .map(|p| p as u16)
        .find(|p| !is_taken(*p))
        .ok_or_else(|| AppError::InvalidInput(format!("No free port found near {preferred}")))
}

/// A port is taken if loopback binding fails because it is in use.
pub fn is_bound(port: u16) -> bool {
    let in_use = |result: std::io::Result<TcpListener>| matches!(&result, Err(e) if e.kind() == std::io::ErrorKind::AddrInUse);
    in_use(TcpListener::bind((Ipv4Addr::LOCALHOST, port)))
        || in_use(TcpListener::bind((Ipv6Addr::LOCALHOST, port)))
        || in_use(TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_taken_ports_and_clamps_to_unprivileged_range() {
        assert_eq!(first_free(3000, |p| p == 3000 || p == 3001).unwrap(), 3002);
        assert_eq!(first_free(80, |_| false).unwrap(), 1024);
        assert!(first_free(3000, |_| true).is_err());
        assert_eq!(first_free(65535, |_| false).unwrap(), 65535);
        assert!(first_free(65535, |_| true).is_err());
    }

    #[test]
    fn real_bound_ports_are_detected() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(is_bound(port));
        drop(listener);
        assert!(!is_bound(port));
    }
}
