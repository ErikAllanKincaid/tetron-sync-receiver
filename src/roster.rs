//! Queries the *local* tetron daemon's own peer roster directly over
//! `tetron-proto`'s IPC socket -- the whole point of this being a standalone
//! binary rather than tetron-webui-internal code: a receiver machine that is
//! itself a mesh node already knows every peer's mesh IP, with no need to
//! go through a browser-polled webui roster at all.
//!
//! Connection pattern mirrors tetron-webui's own `ipc_client.rs`: a fresh
//! connection per call, no persistent connection to detect/recover from a
//! daemon restart.

use tetron_proto::ipc::{self, IpcMessage, PeerStatus};

pub struct Peer {
    pub hostname: Option<String>,
    pub ip: String,
}

/// Every peer across every network this host belongs to, flattened. Good
/// enough for v1 (`allow add-peer <name-or-ip>` picks from this list) --
/// multi-network disambiguation (the same hostname in two networks) is a
/// follow-up if it turns out to matter in practice.
pub async fn list_peers() -> anyhow::Result<Vec<Peer>> {
    let mut stream = ipc::connect()
        .await
        .map_err(|e| anyhow::anyhow!("could not reach the tetron daemon: {e}"))?;
    ipc::send(&mut stream, IpcMessage::Status)
        .await
        .map_err(|e| anyhow::anyhow!("failed to send request to daemon: {e}"))?;
    let resp = ipc::recv(&mut stream)
        .await
        .map_err(|e| anyhow::anyhow!("failed to read daemon response: {e}"))?;

    let IpcMessage::StatusResponse { networks, .. } = resp else {
        anyhow::bail!("unexpected daemon response to Status");
    };

    let mut peers = Vec::new();
    for network in networks {
        for PeerStatus { ip, hostname, .. } in network.peers {
            peers.push(Peer {
                hostname,
                ip: ip.to_string(),
            });
        }
    }
    Ok(peers)
}

/// Resolves a hostname (case-insensitive) to a mesh IP via the roster
/// above, for `allow add-peer <name>`. `allow add <ip>` (main.rs) bypasses
/// this entirely and takes a raw IP -- always works even with no local
/// daemon reachable, which this lookup requires.
pub async fn resolve_hostname(name: &str) -> anyhow::Result<String> {
    let peers = list_peers().await?;
    peers
        .into_iter()
        .find(|p| p.hostname.as_deref().is_some_and(|h| h.eq_ignore_ascii_case(name)))
        .map(|p| p.ip)
        .ok_or_else(|| anyhow::anyhow!("no peer with hostname '{name}' found in the local mesh roster"))
}
