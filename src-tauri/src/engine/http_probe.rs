//! Safe local HTTP detection.
//!
//! Only already-discovered *loopback* listening endpoints are probed, with a
//! short timeout and a minimal request. Redirects are never followed (only the
//! status line is read). A failed probe means "not confirmed HTTP", never an
//! application error.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const PROBE_TIMEOUT: Duration = Duration::from_millis(350);
/// A negative result is retried once after this delay (a server may bind its
/// socket slightly before it can answer).
const RETRY_AFTER: Duration = Duration::from_secs(5);
const MAX_ATTEMPTS: u8 = 2;
const MAX_CONCURRENT_PROBES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeResult {
    Pending,
    Http,
    NotHttp,
}

/// Identity of a probed endpoint: a new owner on the same port is a new probe.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProbeKey {
    pub port: u16,
    pub pids: Vec<u32>,
}

pub trait HttpProber: Send {
    /// Current knowledge about `key`; starts a probe in the background the
    /// first time (never blocks the scan).
    fn probe(&mut self, key: &ProbeKey, target: SocketAddr, now: Instant) -> ProbeResult;
    /// Drop knowledge about endpoints that are no longer listening.
    fn retain(&mut self, live: &HashSet<ProbeKey>);
}

/// Blocking probe of a single loopback endpoint.
pub fn probe_http(target: SocketAddr, timeout: Duration) -> bool {
    if !target.ip().is_loopback() {
        return false;
    }
    let host = format!("localhost:{}", target.port());
    match exchange(target, timeout, "HEAD", &host) {
        Exchange::Http => true,
        Exchange::NotHttp => false,
        // No bytes at all: some servers hang up on HEAD — one minimal GET.
        Exchange::Silent => matches!(exchange(target, timeout, "GET", &host), Exchange::Http),
    }
}

enum Exchange {
    Http,
    NotHttp,
    Silent,
}

fn exchange(target: SocketAddr, timeout: Duration, method: &str, host: &str) -> Exchange {
    let Ok(mut stream) = TcpStream::connect_timeout(&target, timeout) else {
        return Exchange::NotHttp;
    };
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    let request = format!(
        "{method} / HTTP/1.1\r\nHost: {host}\r\nUser-Agent: LocalDeck-probe\r\nAccept: */*\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return Exchange::Silent;
    }
    let mut buffer = [0u8; 16];
    let mut read = 0;
    while read < 9 {
        match stream.read(&mut buffer[read..]) {
            Ok(0) => break,
            Ok(n) => read += n,
            Err(_) => break,
        }
    }
    if read == 0 {
        return Exchange::Silent;
    }
    if is_http_status_line(&buffer[..read]) {
        Exchange::Http
    } else {
        Exchange::NotHttp
    }
}

/// `HTTP/1.x ` or `HTTP/2`-style status line prefix.
fn is_http_status_line(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && bytes.starts_with(b"HTTP/") && bytes[5].is_ascii_digit()
}

/// Connect address for a listening endpoint, or `None` when it must not be probed
/// (not reachable through loopback).
pub fn probe_target(addresses: &[IpAddr], port: u16) -> Option<SocketAddr> {
    let v4 = addresses
        .iter()
        .any(|a| matches!(a, IpAddr::V4(v) if v.is_loopback() || v.is_unspecified()));
    if v4 {
        return Some(SocketAddr::new(
            IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            port,
        ));
    }
    let v6 = addresses
        .iter()
        .any(|a| matches!(a, IpAddr::V6(v) if v.is_loopback() || v.is_unspecified()));
    v6.then(|| SocketAddr::new(IpAddr::V6(std::net::Ipv6Addr::LOCALHOST), port))
}

enum Entry {
    Pending,
    Done {
        result: ProbeResult,
        attempts: u8,
        at: Instant,
    },
}

#[derive(Default)]
struct Shared {
    entries: HashMap<ProbeKey, Entry>,
    in_flight: usize,
}

/// Background-thread prober with a per-endpoint cache.
#[derive(Clone)]
pub struct ThreadedProber {
    shared: Arc<Mutex<Shared>>,
    timeout: Duration,
}

impl ThreadedProber {
    pub fn new() -> Self {
        Self::with_timeout(PROBE_TIMEOUT)
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            shared: Arc::new(Mutex::new(Shared::default())),
            timeout,
        }
    }

    fn spawn(&self, key: ProbeKey, target: SocketAddr, attempts: u8) {
        let shared = self.shared.clone();
        let timeout = self.timeout;
        let spawn_key = key.clone();
        {
            let Ok(mut guard) = self.shared.lock() else {
                return;
            };
            if guard.in_flight >= MAX_CONCURRENT_PROBES {
                // Leave it unset; the next scan tries again.
                guard.entries.remove(&key);
                return;
            }
            guard.in_flight += 1;
            guard.entries.insert(key.clone(), Entry::Pending);
        }
        let spawned = std::thread::Builder::new()
            .name("localdeck-probe".to_string())
            .spawn(move || {
                let ok = probe_http(target, timeout);
                if let Ok(mut guard) = shared.lock() {
                    guard.in_flight = guard.in_flight.saturating_sub(1);
                    // The entry may have been dropped while we were probing.
                    if guard.entries.contains_key(&key) {
                        guard.entries.insert(
                            key,
                            Entry::Done {
                                result: if ok {
                                    ProbeResult::Http
                                } else {
                                    ProbeResult::NotHttp
                                },
                                attempts: attempts + 1,
                                at: Instant::now(),
                            },
                        );
                    }
                }
            });
        if spawned.is_err() {
            if let Ok(mut guard) = self.shared.lock() {
                guard.in_flight = guard.in_flight.saturating_sub(1);
                guard.entries.remove(&spawn_key);
            }
        }
    }
}

impl Default for ThreadedProber {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpProber for ThreadedProber {
    fn probe(&mut self, key: &ProbeKey, target: SocketAddr, now: Instant) -> ProbeResult {
        enum Action {
            Report(ProbeResult),
            Start(u8),
        }
        let action = {
            let Ok(guard) = self.shared.lock() else {
                return ProbeResult::NotHttp;
            };
            match guard.entries.get(key) {
                None => Action::Start(0),
                Some(Entry::Pending) => Action::Report(ProbeResult::Pending),
                Some(Entry::Done {
                    result,
                    attempts,
                    at,
                }) => {
                    if *result == ProbeResult::NotHttp
                        && *attempts < MAX_ATTEMPTS
                        && now.saturating_duration_since(*at) >= RETRY_AFTER
                    {
                        Action::Start(*attempts)
                    } else {
                        Action::Report(*result)
                    }
                }
            }
        };
        match action {
            Action::Report(result) => result,
            Action::Start(attempts) => {
                self.spawn(key.clone(), target, attempts);
                ProbeResult::Pending
            }
        }
    }

    fn retain(&mut self, live: &HashSet<ProbeKey>) {
        if let Ok(mut guard) = self.shared.lock() {
            guard.entries.retain(|key, _| live.contains(key));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr, TcpListener};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    /// Serves `behaviour` for every accepted connection; counts connections.
    fn serve(
        behaviour: impl Fn(usize, TcpStream) + Send + 'static,
    ) -> (SocketAddr, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();
        thread::spawn(move || {
            for stream in listener.incoming().map_while(Result::ok) {
                let n = counter.fetch_add(1, Ordering::SeqCst);
                behaviour(n, stream);
            }
        });
        (addr, count)
    }

    fn drain(stream: &mut TcpStream) {
        let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
        let mut buf = [0u8; 512];
        let _ = stream.read(&mut buf);
    }

    #[test]
    fn confirms_a_real_http_server_including_redirects_and_405() {
        for status in [
            "200 OK",
            "302 Found",
            "405 Method Not Allowed",
            "404 Not Found",
        ] {
            let (addr, _) = serve(move |_, mut s| {
                drain(&mut s);
                let _ = s.write_all(
                    format!("HTTP/1.1 {status}\r\nLocation: http://example.com/\r\nContent-Length: 0\r\n\r\n")
                        .as_bytes(),
                );
            });
            assert!(probe_http(addr, Duration::from_millis(500)), "{status}");
        }
    }

    #[test]
    fn redirects_to_external_hosts_are_never_followed() {
        let (addr, count) = serve(|_, mut s| {
            drain(&mut s);
            let _ = s.write_all(b"HTTP/1.1 301 Moved\r\nLocation: http://203.0.113.9/\r\n\r\n");
        });
        assert!(probe_http(addr, Duration::from_millis(500)));
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "exactly one connection, to the local port"
        );
    }

    #[test]
    fn non_http_protocols_are_not_confirmed() {
        let (addr, _) = serve(|_, mut s| {
            drain(&mut s);
            let _ = s.write_all(b"-ERR unknown command 'HEAD'\r\n");
        });
        assert!(!probe_http(addr, Duration::from_millis(500)));
    }

    #[test]
    fn silent_hangups_get_exactly_one_get_fallback() {
        // First connection (HEAD) is dropped silently; the second (GET) answers.
        let (addr, count) = serve(|n, mut s| {
            drain(&mut s);
            if n >= 1 {
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
            }
        });
        assert!(probe_http(addr, Duration::from_millis(500)));
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn closed_ports_and_slow_servers_fail_quickly() {
        let closed = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = closed.local_addr().unwrap();
        drop(closed);
        assert!(!probe_http(addr, Duration::from_millis(300)));

        let (slow, _) = serve(|_, mut s| {
            drain(&mut s);
            thread::sleep(Duration::from_secs(2));
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
        });
        let started = Instant::now();
        assert!(!probe_http(slow, Duration::from_millis(150)));
        assert!(started.elapsed() < Duration::from_millis(1500));
    }

    #[test]
    fn never_probes_non_loopback_addresses() {
        let lan: SocketAddr = "192.0.2.10:3000".parse().unwrap();
        assert!(!probe_http(lan, Duration::from_millis(50)));
    }

    #[test]
    fn probe_target_prefers_ipv4_loopback_and_skips_lan_only_listeners() {
        let v4 = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
        let v6 = IpAddr::V6(Ipv6Addr::UNSPECIFIED);
        let lan = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5));
        assert_eq!(
            probe_target(&[v6, v4], 3000),
            Some("127.0.0.1:3000".parse().unwrap())
        );
        assert_eq!(
            probe_target(&[IpAddr::V6(Ipv6Addr::LOCALHOST)], 3000),
            Some("[::1]:3000".parse().unwrap())
        );
        assert_eq!(probe_target(&[lan], 3000), None);
    }

    fn wait_for(prober: &mut ThreadedProber, key: &ProbeKey, target: SocketAddr) -> ProbeResult {
        for _ in 0..200 {
            let result = prober.probe(key, target, Instant::now());
            if result != ProbeResult::Pending {
                return result;
            }
            thread::sleep(Duration::from_millis(10));
        }
        ProbeResult::Pending
    }

    #[test]
    fn threaded_prober_probes_once_and_caches() {
        let (addr, count) = serve(|_, mut s| {
            drain(&mut s);
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
        });
        let mut prober = ThreadedProber::with_timeout(Duration::from_millis(500));
        let key = ProbeKey {
            port: addr.port(),
            pids: vec![42],
        };
        assert_eq!(
            prober.probe(&key, addr, Instant::now()),
            ProbeResult::Pending
        );
        assert_eq!(wait_for(&mut prober, &key, addr), ProbeResult::Http);
        for _ in 0..5 {
            assert_eq!(prober.probe(&key, addr, Instant::now()), ProbeResult::Http);
        }
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn new_owner_on_the_same_port_triggers_a_fresh_probe() {
        let (addr, count) = serve(|_, mut s| {
            drain(&mut s);
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
        });
        let mut prober = ThreadedProber::with_timeout(Duration::from_millis(500));
        let first = ProbeKey {
            port: addr.port(),
            pids: vec![1],
        };
        let second = ProbeKey {
            port: addr.port(),
            pids: vec![2],
        };
        wait_for(&mut prober, &first, addr);
        wait_for(&mut prober, &second, addr);
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn negative_results_retry_once_after_the_delay_then_stick() {
        let (addr, count) = serve(|_, mut s| {
            drain(&mut s);
            let _ = s.write_all(b"nope\r\n");
        });
        let mut prober = ThreadedProber::with_timeout(Duration::from_millis(300));
        let key = ProbeKey {
            port: addr.port(),
            pids: vec![9],
        };
        assert_eq!(wait_for(&mut prober, &key, addr), ProbeResult::NotHttp);
        // Too early for a retry.
        assert_eq!(
            prober.probe(&key, addr, Instant::now()),
            ProbeResult::NotHttp
        );
        let later = Instant::now() + RETRY_AFTER + Duration::from_secs(1);
        assert_eq!(prober.probe(&key, addr, later), ProbeResult::Pending);
        for _ in 0..200 {
            if count.load(Ordering::SeqCst) >= 2
                && prober.probe(&key, addr, later) == ProbeResult::NotHttp
            {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            prober.probe(&key, addr, later + RETRY_AFTER * 4),
            ProbeResult::NotHttp
        );
        assert_eq!(count.load(Ordering::SeqCst), 2, "no third attempt");
    }

    #[test]
    fn retain_forgets_closed_endpoints() {
        let (addr, count) = serve(|_, mut s| {
            drain(&mut s);
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\n\r\n");
        });
        let mut prober = ThreadedProber::with_timeout(Duration::from_millis(500));
        let key = ProbeKey {
            port: addr.port(),
            pids: vec![1],
        };
        wait_for(&mut prober, &key, addr);
        prober.retain(&HashSet::new());
        wait_for(&mut prober, &key, addr);
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }
}
