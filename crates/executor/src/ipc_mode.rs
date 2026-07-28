//! Multi-process access policy for a durable-local open.
//!
//! This is an executor (transport-layer) concern, not an engine one: the
//! storage engine admits exactly one read-write process per store and knows
//! nothing about sockets. `IpcMode` selects how the executor's `Connection`
//! behaves when it opens a durable store — whether it hosts a broker socket,
//! brokers to an existing owner, or stays single-process.
//!
//! It lives here (always compiled) rather than in the unix-only `ipc` module so
//! every frontend can name it on all platforms; on a target without the socket
//! transport it simply has no effect.

/// How a durable-local open participates in same-machine multi-process access.
///
/// - `Host` (the default) wins the writer lock and hosts a socket other
///   processes can broker to; on contention it brokers to the existing owner.
/// - `Client` wins the lock but does not host; on contention it brokers to the
///   existing owner as a client (a short-lived one-shot).
/// - `Off` opts out entirely — a raw single-process open with no socket and no
///   broker fallback (hardened deployments).
///
/// Deliberately exhaustive (not `#[non_exhaustive]`): the connection matches on
/// it to route the open, and these three modes are the complete host-or-not ×
/// broker-or-not partition — a future variant must break every routing match
/// loudly rather than fall into a silent default.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum IpcMode {
    /// Win the lock and host a socket for other processes to broker to.
    #[default]
    Host,
    /// Win the lock without hosting; broker to an existing owner on contention.
    Client,
    /// Single-process only: no socket, no broker fallback.
    Off,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_mode_defaults_to_host() {
        // On by default: the socket is the multi-process substrate, so an
        // unspecified open participates as a host rather than opting out.
        assert_eq!(IpcMode::default(), IpcMode::Host);
    }
}
