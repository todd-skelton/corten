//! The address of who is asking, for an app that passes it on (to an API's per-client limit) or
//! limits by it itself.
//!
//! [`ClientIp`] is the connection's peer, or behind a proxy the app trusts
//! ([`Policy::proxy`](crate::security::Policy::proxy), `PROXIES`) the nearest address in
//! `X-Forwarded-For` that is not one of its proxies. Only that header is read: a proxy sets one
//! of `X-Forwarded-For` and `Forwarded`, and reading the other would take whatever a client
//! wrote in it.

use std::{
    hash::{Hash, Hasher},
    net::{IpAddr, SocketAddr},
};

use axum::{
    extract::{
        connect_info::{ConnectInfo, MockConnectInfo},
        FromRequestParts, OptionalFromRequestParts,
    },
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
};

use crate::{Error, Signal};

/// The IP address of who is asking: `async fn signup(ClientIp(ip): ClientIp, ..)`.
///
/// Without a trusted proxy it is the connection's peer. With one
/// ([`Policy::proxy`](crate::security::Policy::proxy), or `PROXIES` where the deploy sets `HOST`
/// and `PORT`), a request from a proxy is followed back through `X-Forwarded-For`, right to left,
/// to the first address that is not a proxy: entries further left are whatever the client sent.
/// A request on a Unix socket counts as from a proxy when the app has any, since only this
/// machine reaches the socket. IPv4 addresses come as IPv4, also when a dual-stack socket
/// reports them as `::ffff:a.b.c.d`.
///
/// It tells who is asking, so a [`Public`](crate::Public) handler cannot take it, and an answer
/// that read it is private to that address:
///
/// ```compile_fail,E0277
/// use std::time::Duration;
/// use corten::{prelude::*, ClientIp};
///
/// #[corten::routes]
/// mod url {
///     #[route("/", get = home)]
///     pub struct Home;
/// }
///
/// async fn home(ClientIp(ip): ClientIp) -> Public<String> {
///     Public::new(Duration::from_secs(60), ip.to_string())
/// }
/// # fn main() { let _ = url::router(); }
/// ```
///
/// When the address cannot be known (a Unix socket and no proxy, a listener whose address is not
/// a socket address, a test's `oneshot`) taking `ClientIp` is a 500 and `Option<ClientIp>` is
/// `None`. A test gives the request one with axum's `MockConnectInfo` layer (or a
/// `ConnectInfo<SocketAddr>` extension).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClientIp(pub IpAddr);

impl ClientIp {
    /// Looked up once per request: a second read must not mark the caller again.
    fn find(parts: &mut Parts) -> Option<Self> {
        if let Some(Found(found)) = parts.extensions.get::<Found>() {
            return *found;
        }
        // A test's address first, then the connection's.
        let peer = match parts.extensions.get::<ConnectInfo<SocketAddr>>() {
            Some(ConnectInfo(peer)) => Peer::Ip(peer.ip()),
            None => match parts.extensions.get::<MockConnectInfo<SocketAddr>>() {
                Some(MockConnectInfo(peer)) => Peer::Ip(peer.ip()),
                None => PEER.get(),
            },
        };
        let signal = parts.extensions.get::<Signal>();
        let proxies = signal.and_then(Signal::shared).map_or(&[][..], |shared| &shared.proxies[..]);
        let forwarded = parts.headers.get_all("x-forwarded-for").iter().rev().flat_map(|value| {
            // A value that is not text ends the walk where it is, like an entry that is not an address.
            value.to_str().map_or_else(|_| "?".split(','), |value| value.split(',')).rev()
        });
        let found = client(peer, forwarded, proxies).map(ClientIp);
        // The answer is for this address: private, and keyed by it like a caller.
        if let (Some(ClientIp(ip)), Some(signal)) = (found, signal) {
            let mut hasher = std::hash::DefaultHasher::new();
            "corten::ClientIp".hash(&mut hasher);
            ip.hash(&mut hasher);
            signal.mark_caller(hasher.finish());
        }
        parts.extensions.insert(Found(found));
        found
    }
}

/// The request's [`ClientIp`], once looked up.
#[derive(Clone, Copy)]
struct Found(Option<ClientIp>);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Response> {
        ClientIp::find(parts).ok_or_else(|| {
            Error::new(
                "corten::ClientIp: the request has no client address (a Unix socket with no Policy::proxy, a listener \
                 without socket addresses, or a test without ConnectInfo); take Option<ClientIp> where none is an answer",
            )
            .with_status(StatusCode::INTERNAL_SERVER_ERROR)
            .into_response()
        })
    }
}

impl<S: Send + Sync> OptionalFromRequestParts<S> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Option<Self>, Self::Rejection> {
        Ok(ClientIp::find(parts))
    }
}

/// Where a connection came from, by its listener's address type.
#[derive(Clone, Copy)]
pub(crate) enum Peer {
    Ip(IpAddr),
    /// A Unix socket: only this machine reaches it.
    #[cfg_attr(not(unix), allow(dead_code))]
    Local,
    /// A listener of the app's own whose address is neither.
    Unknown,
}

std::thread_local! {
    /// The peer of the request being polled on this thread, while [`WithPeer`] polls it.
    static PEER: std::cell::Cell<Peer> = const { std::cell::Cell::new(Peer::Unknown) };
}

/// A request's future with its connection's peer at hand while it is polled. What the request
/// moves to another task (a layer that buffers it) does not have it there.
pub(crate) struct WithPeer<F> {
    peer: Peer,
    inner: F,
}

impl<F> WithPeer<F> {
    pub(crate) fn new(peer: Peer, inner: F) -> Self {
        WithPeer { peer, inner }
    }
}

impl<F: std::future::Future + Unpin> std::future::Future for WithPeer<F> {
    type Output = F::Output;

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<F::Output> {
        /// Puts back what the thread had, also when the poll panics.
        struct Restore(Peer);
        impl Drop for Restore {
            fn drop(&mut self) {
                PEER.set(self.0);
            }
        }
        let _restore = Restore(PEER.replace(self.peer));
        std::pin::Pin::new(&mut self.inner).poll(cx)
    }
}

/// The client: the peer unless it is a trusted proxy (or local, with proxies), then the nearest
/// `forwarded` entry (right to left) that is not one. An entry that is not an address stops the
/// walk at the last address read.
fn client<'a>(peer: Peer, forwarded: impl Iterator<Item = &'a str>, proxies: &[Network]) -> Option<IpAddr> {
    let trusted = |ip: IpAddr| proxies.iter().any(|network| network.contains(ip));
    let mut last = match peer {
        Peer::Ip(ip) => Some(ip.to_canonical()),
        Peer::Local => None,
        Peer::Unknown => return None,
    };
    if proxies.is_empty() || last.is_some_and(|ip| !trusted(ip)) {
        return last;
    }
    for entry in forwarded {
        let Some(ip) = address(entry.trim()) else { return last };
        if !trusted(ip) {
            return Some(ip);
        }
        last = Some(ip);
    }
    last
}

/// An `X-Forwarded-For` entry: an address, with a port or in brackets as some proxies write it.
fn address(entry: &str) -> Option<IpAddr> {
    let ip = entry
        .parse::<IpAddr>()
        .ok()
        .or_else(|| entry.parse::<SocketAddr>().ok().map(|addr| addr.ip()))
        .or_else(|| entry.strip_prefix('[')?.strip_suffix(']')?.parse().ok())?;
    Some(ip.to_canonical())
}

/// An address or a network of them: `10.0.0.0/8`, `127.0.0.1`, `fc00::/7`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Network {
    ip: IpAddr,
    bits: u8,
}

impl Network {
    /// `text` as a network, or `None` when it is not one (a host bit set counts as not one, so a
    /// typo does not trust a wider network than meant).
    pub(crate) fn parse(text: &str) -> Option<Network> {
        let (ip, bits) = match text.split_once('/') {
            Some((ip, bits)) => (ip, Some(bits)),
            None => (text, None),
        };
        let ip = ip.parse::<IpAddr>().ok()?.to_canonical();
        let max = if ip.is_ipv4() { 32 } else { 128 };
        let bits = match bits {
            Some(bits) if !bits.is_empty() && bits.bytes().all(|b| b.is_ascii_digit()) => bits.parse::<u8>().ok()?,
            Some(_) => return None,
            None => max,
        };
        let network = Network { ip, bits };
        (bits <= max && network.masked(ip) == bits_of(ip)).then_some(network)
    }

    fn contains(&self, ip: IpAddr) -> bool {
        self.ip.is_ipv4() == ip.is_ipv4() && self.masked(ip) == bits_of(self.ip)
    }

    /// `ip`'s bits with those past the prefix cleared.
    fn masked(&self, ip: IpAddr) -> u128 {
        let width = if ip.is_ipv4() { 32 } else { 128 };
        let keep = u32::from(self.bits);
        let mask = if keep == 0 { 0 } else { u128::MAX << (width - keep) };
        bits_of(ip) & (mask & (u128::MAX >> (128 - width)))
    }
}

fn bits_of(ip: IpAddr) -> u128 {
    match ip {
        IpAddr::V4(ip) => u128::from(u32::from(ip)),
        IpAddr::V6(ip) => u128::from(ip),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nets(list: &[&str]) -> Vec<Network> {
        list.iter().map(|n| Network::parse(n).unwrap()).collect()
    }

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    fn of(peer: Peer, forwarded: &str, proxies: &[&str]) -> Option<IpAddr> {
        client(peer, forwarded.split(',').rev().filter(|e| !e.is_empty()), &nets(proxies))
    }

    #[test]
    fn networks_parse_as_written() {
        assert!(Network::parse("10.0.0.0/8").is_some());
        assert!(Network::parse("127.0.0.1").is_some());
        assert!(Network::parse("fc00::/7").is_some());
        assert!(Network::parse("0.0.0.0/0").is_some());
        for wrong in ["10.0.0.1/8", "10.0.0.0/33", "10.0.0.0/", "10.0.0.0/+8", "shop.example", "::1/129", ""] {
            assert_eq!(Network::parse(wrong), None, "{wrong}");
        }
        let private = Network::parse("172.16.0.0/12").unwrap();
        assert!(private.contains(ip("172.31.255.255")));
        assert!(!private.contains(ip("172.32.0.0")));
        assert!(!private.contains(ip("::ffff:172.16.0.1")), "callers canonicalize first");
        assert!(Network::parse("fc00::/7").unwrap().contains(ip("fd12::1")));
        assert!(Network::parse("::ffff:10.0.0.0/104").is_none(), "a mapped network is read as IPv4, so /104 is too long");
        assert!(Network::parse("::ffff:10.0.0.1").unwrap().contains(ip("10.0.0.1")));
    }

    #[test]
    fn without_proxies_the_peer_is_the_client() {
        let peer = Peer::Ip(ip("203.0.113.9"));
        assert_eq!(of(peer, "198.51.100.1", &[]), Some(ip("203.0.113.9")));
        assert_eq!(of(Peer::Ip(ip("::ffff:203.0.113.9")), "", &[]), Some(ip("203.0.113.9")));
        assert_eq!(of(Peer::Local, "198.51.100.1", &[]), None);
        assert_eq!(of(Peer::Unknown, "198.51.100.1", &["0.0.0.0/0"]), None);
    }

    #[test]
    fn a_peer_that_is_not_a_proxy_is_the_client_whatever_it_forwards() {
        assert_eq!(of(Peer::Ip(ip("203.0.113.9")), "198.51.100.1", &["10.0.0.0/8"]), Some(ip("203.0.113.9")));
    }

    #[test]
    fn behind_proxies_the_nearest_address_that_is_not_one() {
        let proxies = ["10.0.0.0/8", "fc00::/7"];
        let peer = Peer::Ip(ip("10.0.0.2"));
        // The client wrote 1.1.1.1 itself; 198.51.100.7 is what the first proxy saw.
        assert_eq!(of(peer, "1.1.1.1, 198.51.100.7, 10.0.0.1", &proxies), Some(ip("198.51.100.7")));
        assert_eq!(of(peer, "198.51.100.7:4711", &proxies), Some(ip("198.51.100.7")));
        assert_eq!(of(peer, "[2001:db8::7]:4711", &proxies), Some(ip("2001:db8::7")));
        assert_eq!(of(peer, "[2001:db8::7]", &proxies), Some(ip("2001:db8::7")));
        assert_eq!(of(peer, "::ffff:198.51.100.7", &proxies), Some(ip("198.51.100.7")));
        // Only proxies: the furthest one is as near as it goes.
        assert_eq!(of(peer, "10.0.0.9, 10.0.0.1", &proxies), Some(ip("10.0.0.9")));
        // No header: the proxy itself.
        assert_eq!(of(peer, "", &proxies), Some(ip("10.0.0.2")));
        // An entry that is not an address stops at the last address read.
        assert_eq!(of(peer, "198.51.100.7, unknown, 10.0.0.1", &proxies), Some(ip("10.0.0.1")));
        assert_eq!(of(Peer::Local, "198.51.100.7", &proxies), Some(ip("198.51.100.7")));
        assert_eq!(of(Peer::Local, "", &proxies), None);
    }
}
