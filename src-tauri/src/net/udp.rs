//! UDP 发送套接字：单播与组播。

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};

use serde::Serialize;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use thiserror::Error;

use crate::config::{TargetConfig, TargetKind};

/// UDP 单包数据上限（65535 - 8 字节 UDP 头 - 20 字节 IP 头）
pub const MAX_UDP_PAYLOAD: usize = 65507;

#[derive(Debug, Error)]
pub enum NetError {
    #[error("地址解析失败：{0}")]
    Resolve(String),

    #[error("{0} 不是组播地址，组播地址范围是 224.0.0.0 ~ 239.255.255.255")]
    NotMulticast(String),

    #[error("组播出站网卡地址无效：{0}")]
    BadInterface(String),

    #[error("组播目前仅支持 IPv4")]
    MulticastIpv6Unsupported,

    #[error("绑定本地地址 {addr} 失败：{source}")]
    Bind {
        addr: String,
        #[source]
        source: io::Error,
    },

    #[error("套接字配置失败：{0}")]
    Socket(#[from] io::Error),
}

/// 一次发送尝试的失败原因。区分它们是必要的 —— 处理方式完全不同。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendFail {
    /// 内核发送缓冲已满。高速发送时必然遇到，重试无果后只能丢弃并计数。
    BufferFull,
    /// 收到 ICMP 端口不可达。UDP 语义下继续发，但说明对端多半没在监听。
    Refused,
    /// 其他 IO 错误
    Io(io::ErrorKind),
}

pub struct UdpSender {
    socket: Socket,
    /// 目标描述，供界面与日志展示
    pub description: String,
}

impl std::fmt::Debug for UdpSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UdpSender")
            .field("description", &self.description)
            .field("local", &self.local_addr())
            .finish()
    }
}

impl UdpSender {
    pub fn build(cfg: &TargetConfig) -> Result<Self, NetError> {
        let dest = resolve_dest(&cfg.kind)?;

        let domain = if dest.is_ipv4() {
            Domain::IPV4
        } else {
            Domain::IPV6
        };
        let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;

        bind_local(&socket, cfg, dest.is_ipv4())?;

        if let Some(sz) = cfg.send_buffer_bytes {
            // 失败不致命：多数系统对上限有限制，调不上去就按原样发
            let _ = socket.set_send_buffer_size(sz);
        }

        if let TargetKind::Multicast {
            interface,
            ttl,
            loopback,
            ..
        } = &cfg.kind
        {
            configure_multicast(&socket, dest, interface.as_deref(), *ttl, *loopback)?;
        }

        // connect 之后用 send 而非 send_to：省去每包重复的地址解析，
        // 并且能收到 ICMP 端口不可达转成的 ECONNREFUSED。
        socket.connect(&SockAddr::from(dest))?;
        socket.set_nonblocking(true)?;

        Ok(UdpSender {
            socket,
            description: describe(&cfg.kind, dest),
        })
    }

    /// 发送一帧。缓冲满时退避重试若干次，仍不成功则报 `BufferFull`。
    pub fn send(&self, buf: &[u8], retries: u32) -> Result<usize, SendFail> {
        let mut attempt = 0;
        loop {
            match self.socket.send(buf) {
                Ok(n) => return Ok(n),
                Err(e) => match e.kind() {
                    io::ErrorKind::WouldBlock => {
                        if attempt >= retries {
                            return Err(SendFail::BufferFull);
                        }
                        attempt += 1;
                        std::thread::yield_now();
                    }
                    io::ErrorKind::ConnectionRefused => return Err(SendFail::Refused),
                    // 上一次发送触发的 ICMP 错误可能延迟到本次返回，重试一次即可
                    io::ErrorKind::Interrupted => continue,
                    k => return Err(SendFail::Io(k)),
                },
            }
        }
    }

    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.socket.local_addr().ok().and_then(|a| a.as_socket())
    }
}

fn resolve_dest(kind: &TargetKind) -> Result<SocketAddr, NetError> {
    match kind {
        TargetKind::Unicast { host, port } => (host.as_str(), *port)
            .to_socket_addrs()
            .map_err(|e| NetError::Resolve(format!("{host}:{port} — {e}")))?
            .next()
            .ok_or_else(|| NetError::Resolve(format!("{host}:{port} 未解析到任何地址"))),

        TargetKind::Multicast { group, port, .. } => {
            let ip: IpAddr = group
                .parse()
                .map_err(|_| NetError::Resolve(format!("组播地址 {group} 格式无效")))?;
            match ip {
                IpAddr::V4(v4) => {
                    if !v4.is_multicast() {
                        return Err(NetError::NotMulticast(group.clone()));
                    }
                    Ok(SocketAddr::new(ip, *port))
                }
                IpAddr::V6(_) => Err(NetError::MulticastIpv6Unsupported),
            }
        }
    }
}

fn bind_local(socket: &Socket, cfg: &TargetConfig, ipv4: bool) -> Result<(), NetError> {
    let default_addr: IpAddr = if ipv4 {
        Ipv4Addr::UNSPECIFIED.into()
    } else {
        std::net::Ipv6Addr::UNSPECIFIED.into()
    };

    let addr: IpAddr = match &cfg.bind_addr {
        Some(s) if !s.trim().is_empty() => s
            .parse()
            .map_err(|_| NetError::Resolve(format!("本地绑定地址 {s} 格式无效")))?,
        _ => default_addr,
    };

    let local = SocketAddr::new(addr, cfg.bind_port.unwrap_or(0));
    socket.bind(&SockAddr::from(local)).map_err(|e| NetError::Bind {
        addr: local.to_string(),
        source: e,
    })
}

fn configure_multicast(
    socket: &Socket,
    dest: SocketAddr,
    interface: Option<&str>,
    ttl: u32,
    loopback: bool,
) -> Result<(), NetError> {
    if !dest.is_ipv4() {
        return Err(NetError::MulticastIpv6Unsupported);
    }

    let iface: Ipv4Addr = match interface {
        Some(s) if !s.trim().is_empty() => s
            .parse()
            .map_err(|_| NetError::BadInterface(s.to_string()))?,
        _ => Ipv4Addr::UNSPECIFIED,
    };

    socket.set_multicast_if_v4(&iface)?;
    socket.set_multicast_ttl_v4(ttl)?;
    socket.set_multicast_loop_v4(loopback)?;
    Ok(())
}

fn describe(kind: &TargetKind, dest: SocketAddr) -> String {
    match kind {
        TargetKind::Unicast { .. } => format!("单播 → {dest}"),
        TargetKind::Multicast {
            interface, ttl, ..
        } => {
            let via = interface
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("系统默认");
            format!("组播 → {dest}（网卡 {via}，TTL {ttl}）")
        }
    }
}

// ── 网卡枚举 ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceInfo {
    pub name: String,
    pub ip: String,
    pub is_loopback: bool,
}

/// 列出本机 IPv4 网卡，供组播出站网卡下拉选择。
pub fn list_interfaces() -> Vec<InterfaceInfo> {
    let mut out: Vec<InterfaceInfo> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| i.addr.ip().is_ipv4())
        .map(|i| InterfaceInfo {
            name: i.name.clone(),
            ip: i.addr.ip().to_string(),
            is_loopback: i.is_loopback(),
        })
        .collect();

    // 回环排最后：组播几乎不会想从回环出去
    out.sort_by_key(|i| (i.is_loopback, i.name.clone()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unicast(host: &str, port: u16) -> TargetConfig {
        TargetConfig {
            kind: TargetKind::Unicast {
                host: host.into(),
                port,
            },
            ..Default::default()
        }
    }

    fn multicast(group: &str, port: u16) -> TargetConfig {
        TargetConfig {
            kind: TargetKind::Multicast {
                group: group.into(),
                port,
                interface: None,
                ttl: 1,
                loopback: true,
            },
            ..Default::default()
        }
    }

    #[test]
    fn builds_unicast_socket() {
        let s = UdpSender::build(&unicast("127.0.0.1", 19000)).unwrap();
        assert!(s.description.starts_with("单播 →"));
        assert!(s.local_addr().is_some());
    }

    #[test]
    fn builds_multicast_socket() {
        let s = UdpSender::build(&multicast("239.255.0.1", 19001)).unwrap();
        assert!(s.description.starts_with("组播 →"));
    }

    #[test]
    fn rejects_non_multicast_group() {
        let err = UdpSender::build(&multicast("192.168.1.1", 19002)).unwrap_err();
        assert!(matches!(err, NetError::NotMulticast(_)));
    }

    #[test]
    fn rejects_malformed_group() {
        let err = UdpSender::build(&multicast("not-an-ip", 19003)).unwrap_err();
        assert!(matches!(err, NetError::Resolve(_)));
    }

    #[test]
    fn honours_explicit_bind_port() {
        let cfg = TargetConfig {
            bind_port: Some(19100),
            ..unicast("127.0.0.1", 19004)
        };
        let s = UdpSender::build(&cfg).unwrap();
        assert_eq!(s.local_addr().unwrap().port(), 19100);
    }

    #[test]
    fn lists_at_least_loopback_interface() {
        let ifs = list_interfaces();
        assert!(ifs.iter().any(|i| i.is_loopback));
    }
}
