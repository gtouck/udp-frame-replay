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

/// 每包的投递方式。单播与组播必须分开处理，理由见 `UdpSender::build`。
enum Dispatch {
    /// 已 connect，每包 `send`
    Connected,
    /// 未 connect，每包 `send_to`
    To(SockAddr),
}

pub struct UdpSender {
    socket: Socket,
    dispatch: Dispatch,
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

        let dispatch = match &cfg.kind {
            TargetKind::Multicast {
                interface,
                ttl,
                loopback,
                ..
            } => {
                let iface = multicast_if(interface.as_deref(), cfg.bind_addr.as_deref())?;
                configure_multicast(&socket, dest, iface, *ttl, *loopback)?;

                // 组播绝不能 connect，两个原因：
                //
                // 1. connect 做的是**单播**路由查找，完全不看 IP_MULTICAST_IF。
                //    出站网卡本该由上面的 set_multicast_if_v4 决定，connect 却会
                //    把路由按单播规则定死并缓存下来，之后再改 IP_MULTICAST_IF 也没用。
                //
                // 2. BSD/macOS 上，已 connect 的 UDP 套接字发送失败时错误会被闩存进
                //    so_error，下一次 send 统一返回 EPIPE —— 真实原因（无路由、网卡
                //    不可用、macOS 本地网络权限未授予都会报 EHOSTUNREACH）被替换成
                //    一个毫无信息量的 BrokenPipe，日志上完全无从下手。
                //
                // 不 connect 改用 send_to，两个问题一起消失：错误码是真的，
                // 且换到可用网卡后立刻恢复。ECONNREFUSED 那点好处对组播本就不成立
                // —— 组播没有单一对端，谈不上"端口不可达"。
                Dispatch::To(SockAddr::from(dest))
            }

            // 单播保留 connect：省去每包重复的地址解析，
            // 并且能收到 ICMP 端口不可达转成的 ECONNREFUSED。
            TargetKind::Unicast { .. } => {
                socket.connect(&SockAddr::from(dest))?;
                Dispatch::Connected
            }
        };

        socket.set_nonblocking(true)?;

        Ok(UdpSender {
            socket,
            dispatch,
            description: describe(cfg, dest),
        })
    }

    /// 发送一帧。缓冲满时退避重试若干次，仍不成功则报 `BufferFull`。
    pub fn send(&self, buf: &[u8], retries: u32) -> Result<usize, SendFail> {
        let mut attempt = 0;
        loop {
            let result = match &self.dispatch {
                Dispatch::Connected => self.socket.send(buf),
                Dispatch::To(dest) => self.socket.send_to(buf, dest),
            };
            match result {
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

    /// 已 connect 的对端地址；未 connect（组播）时为 `None`。
    pub fn peer_addr(&self) -> Option<SocketAddr> {
        self.socket.peer_addr().ok().and_then(|a| a.as_socket())
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

/// 组播的出站网卡。
///
/// bind 只决定**源地址**，不决定组播从哪张网卡出去 —— 出口由 `IP_MULTICAST_IF`
/// 单独控制，不设就按路由表挑，多网卡机器上很容易挑错。所以这里让它默认跟随
/// 本地 IP：界面上选了本地 IP，组播就从那张网卡出去，符合直觉。
/// 配置里显式写了 interface（老配置档）仍然优先。
fn multicast_if(interface: Option<&str>, bind_addr: Option<&str>) -> Result<Ipv4Addr, NetError> {
    let nonempty = |s: &&str| !s.trim().is_empty();

    if let Some(s) = interface.filter(nonempty) {
        return s
            .trim()
            .parse()
            .map_err(|_| NetError::BadInterface(s.to_string()));
    }

    // 本地 IP 的格式错误留给 bind 去报，这里解析不出来就当没设
    Ok(bind_addr
        .filter(nonempty)
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(Ipv4Addr::UNSPECIFIED))
}

fn configure_multicast(
    socket: &Socket,
    dest: SocketAddr,
    iface: Ipv4Addr,
    ttl: u32,
    loopback: bool,
) -> Result<(), NetError> {
    if !dest.is_ipv4() {
        return Err(NetError::MulticastIpv6Unsupported);
    }

    socket.set_multicast_if_v4(&iface)?;
    socket.set_multicast_ttl_v4(ttl)?;
    socket.set_multicast_loop_v4(loopback)?;
    Ok(())
}

fn describe(cfg: &TargetConfig, dest: SocketAddr) -> String {
    match &cfg.kind {
        TargetKind::Unicast { .. } => format!("单播 → {dest}"),
        TargetKind::Multicast {
            interface, ttl, ..
        } => {
            let via = match multicast_if(interface.as_deref(), cfg.bind_addr.as_deref()) {
                Ok(ip) if !ip.is_unspecified() => ip.to_string(),
                _ => "系统默认".to_string(),
            };
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

/// 列出本机 IPv4 网卡，供本地绑定地址与组播出站网卡下拉选择。
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

    /// 组播绝不能 connect：connect 的路由查找不看 `IP_MULTICAST_IF`，
    /// 而且 BSD/macOS 会把发送失败闩存进 `so_error`，之后每次 send 都返回
    /// EPIPE（BrokenPipe），真实错误被彻底掩盖。
    #[test]
    fn multicast_socket_is_not_connected() {
        let s = UdpSender::build(&multicast("239.255.0.1", 19010)).unwrap();
        assert!(s.peer_addr().is_none(), "组播套接字不应 connect");
    }

    /// 单播保留 connect：能把 ICMP 端口不可达转成 ECONNREFUSED，这对单播有意义。
    #[test]
    fn unicast_socket_is_connected() {
        let s = UdpSender::build(&unicast("127.0.0.1", 19011)).unwrap();
        assert_eq!(s.peer_addr().map(|a| a.port()), Some(19011));
    }

    /// 回归：组播发送失败时必须报出真实原因。
    /// 组播能不能发出去取决于运行环境（macOS 本地网络权限、路由、网卡），
    /// 所以这里只断言"要么成功，要么给出真实错误"——唯独不能是 BrokenPipe。
    #[test]
    fn multicast_failure_is_not_masked_as_broken_pipe() {
        let s = UdpSender::build(&multicast("239.255.0.1", 19012)).unwrap();
        if let Err(SendFail::Io(kind)) = s.send(&[0xAA; 32], 3) {
            assert_ne!(
                kind,
                io::ErrorKind::BrokenPipe,
                "BrokenPipe 说明真实错误被 connect 掩盖了"
            );
        }
    }

    /// 没单独指定出站网卡时，组播跟着本地 IP 走 —— 界面上只剩「本地 IP」一个旋钮。
    #[test]
    fn multicast_follows_bind_address() {
        let cfg = TargetConfig {
            bind_addr: Some("127.0.0.1".into()),
            ..multicast("239.255.0.1", 19013)
        };
        let s = UdpSender::build(&cfg).unwrap();
        assert_eq!(s.socket.multicast_if_v4().unwrap(), Ipv4Addr::LOCALHOST);
    }

    #[test]
    fn explicit_interface_beats_bind_address() {
        let iface = multicast_if(Some("10.0.0.5"), Some("127.0.0.1")).unwrap();
        assert_eq!(iface, Ipv4Addr::new(10, 0, 0, 5));
        assert!(multicast_if(None, None).unwrap().is_unspecified());
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
    fn honours_explicit_bind_address() {
        let cfg = TargetConfig {
            bind_addr: Some("127.0.0.1".into()),
            ..unicast("127.0.0.1", 19005)
        };
        let s = UdpSender::build(&cfg).unwrap();
        assert_eq!(s.local_addr().unwrap().ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn lists_at_least_loopback_interface() {
        let ifs = list_interfaces();
        assert!(ifs.iter().any(|i| i.is_loopback));
    }
}
