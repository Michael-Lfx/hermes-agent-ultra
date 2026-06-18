//! Lightweight domestic endpoint reachability checks (UZI `network_preflight.py` subset).

use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;

use tracing::warn;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// TCP connect probe for a host:443 (HTTPS).
#[must_use]
pub fn domestic_reachable(host: &str) -> bool {
    let target = format!("{host}:443");
    let Ok(addrs) = target.to_socket_addrs() else {
        return false;
    };
    for addr in addrs {
        if tcp_connect(addr) {
            return true;
        }
    }
    false
}

fn tcp_connect(addr: SocketAddr) -> bool {
    std::net::TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).is_ok()
}

/// Log push2 / tencent reachability when quote chain fails (non-blocking diagnostic).
pub fn log_domestic_diagnostic() {
    let push2 = domestic_reachable("push2.eastmoney.com");
    let tencent = domestic_reachable("qt.gtimg.cn");
    warn!(
        push2_reachable = push2,
        tencent_reachable = tencent,
        "A-share quote chain failed; domestic endpoint reachability"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domestic_reachable_localhost_false_for_random_port() {
        assert!(!domestic_reachable("127.0.0.1.invalid.example"));
    }
}
