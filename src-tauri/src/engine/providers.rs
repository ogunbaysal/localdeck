//! `sysinfo` / `netstat2` backed providers. All third-party types stop here.

use super::raw::{ProcessProvider, RawProcess, RawSocket, SocketProvider, SocketScan};
use crate::error::{AppError, AppResult};
use netstat2::{
    iterate_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState,
};
use std::collections::BTreeMap;
use std::net::IpAddr;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

pub struct SysinfoProcessProvider {
    system: System,
}

impl SysinfoProcessProvider {
    pub fn new() -> Self {
        Self {
            system: System::new(),
        }
    }

    fn refresh_kind() -> ProcessRefreshKind {
        ProcessRefreshKind::nothing()
            .with_cpu()
            .with_memory()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .with_cwd(UpdateKind::OnlyIfNotSet)
            .without_tasks()
    }
}

impl Default for SysinfoProcessProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessProvider for SysinfoProcessProvider {
    fn snapshot(&mut self) -> AppResult<Vec<RawProcess>> {
        self.system
            .refresh_processes_specifics(ProcessesToUpdate::All, true, Self::refresh_kind());
        let processes = self
            .system
            .processes()
            .iter()
            .map(|(pid, process)| {
                let executable = process.exe().map(|p| p.to_path_buf());
                let args: Vec<String> = process
                    .cmd()
                    .iter()
                    .map(|a| a.to_string_lossy().to_string())
                    .collect();
                let cwd = process.cwd().map(|p| p.to_path_buf());
                let start = process.start_time();
                RawProcess {
                    pid: pid.as_u32(),
                    parent_pid: process.parent().map(|p| p.as_u32()),
                    name: process.name().to_string_lossy().to_string(),
                    access: RawProcess::access_from(
                        executable.is_some(),
                        !args.is_empty(),
                        cwd.is_some(),
                    ),
                    executable,
                    args,
                    cwd,
                    cpu_usage: process.cpu_usage(),
                    memory_bytes: process.memory(),
                    started_at: (start > 0).then(|| (start as i64).saturating_mul(1000)),
                }
            })
            .collect();
        Ok(processes)
    }
}

pub struct NetstatSocketProvider;

impl SocketProvider for NetstatSocketProvider {
    fn listening_sockets(&mut self) -> AppResult<SocketScan> {
        let iter = iterate_sockets_info(
            AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6,
            ProtocolFlags::TCP,
        )
        .map_err(|e| AppError::PortScanFailed(e.to_string()))?;

        // Some platforms report one row per (pid, fd); merge PIDs per endpoint.
        let mut merged: BTreeMap<(IpAddr, u16), Vec<u32>> = BTreeMap::new();
        let mut skipped = 0u32;
        for item in iter {
            let socket = match item {
                Ok(socket) => socket,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            let ProtocolSocketInfo::Tcp(tcp) = socket.protocol_socket_info else {
                continue;
            };
            if tcp.state != TcpState::Listen {
                continue;
            }
            let pids = merged.entry((tcp.local_addr, tcp.local_port)).or_default();
            for pid in socket.associated_pids {
                if !pids.contains(&pid) {
                    pids.push(pid);
                }
            }
        }
        let sockets = merged
            .into_iter()
            .map(|((address, port), mut pids)| {
                pids.sort_unstable();
                RawSocket {
                    address,
                    port,
                    pids,
                }
            })
            .collect();
        Ok(SocketScan { sockets, skipped })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn process_provider_sees_this_process_with_metadata() {
        let mut provider = SysinfoProcessProvider::new();
        let processes = provider.snapshot().unwrap();
        let me = processes
            .iter()
            .find(|p| p.pid == std::process::id())
            .expect("current process is listed");
        assert!(me.parent_pid.is_some());
        assert!(me.memory_bytes > 0);
        assert!(me.cwd.is_some(), "own cwd is readable");
        assert!(me.started_at.is_some());
    }

    #[test]
    fn socket_provider_attributes_a_real_listener_to_this_pid() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let scan = NetstatSocketProvider.listening_sockets().unwrap();
        let ours = scan
            .sockets
            .iter()
            .find(|s| s.port == port)
            .expect("listener is discovered");
        assert!(ours.pids.contains(&std::process::id()));
        drop(listener);
    }

    #[test]
    fn socket_provider_returns_only_listening_endpoints() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let _client = std::net::TcpStream::connect(addr).unwrap();
        let scan = NetstatSocketProvider.listening_sockets().unwrap();
        let matches = scan
            .sockets
            .iter()
            .filter(|s| s.port == addr.port())
            .count();
        assert_eq!(matches, 1, "established connections are not listed");
    }
}
