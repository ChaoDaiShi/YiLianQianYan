//! Per-session forward proxy. No system proxy or persistent credentials are changed.
//! Every TCP target is resolved and checked before connecting; CONNECT and WebSocket
//! upgrades use the same policy. The WebView MUST disable implicit loopback bypass,
//! QUIC and non-proxied WebRTC. Native acceptance tests verify that configuration.
use std::{
    net::Ipv4Addr,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::{
    net::TcpListener,
    sync::{watch, Semaphore},
    time::timeout,
};

#[path = "network/http.rs"]
mod http;
#[path = "network/policy.rs"]
mod policy;
use http::serve;
use policy::{checked_url, explicit_local_host, permitted, Policy};

const HEADER_LIMIT: usize = 32 * 1024;
const BODY_LIMIT: usize = 16 * 1024 * 1024;
const HEADER_TIMEOUT: Duration = Duration::from_secs(8);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

/// Own one proxy per browser session/profile. Dropping it closes its listener and
/// live HTTP, HTTPS and WebSocket connections. It never installs a global proxy.
pub struct BrowserNetworkProxy {
    port: u16,
    policy: Arc<RwLock<Policy>>,
    stop: watch::Sender<bool>,
    changed: watch::Sender<u64>,
}
pub type BrowserProxy = BrowserNetworkProxy;

pub async fn start(
    origins: Vec<String>,
    forbidden_ports: Vec<u16>,
) -> Result<BrowserProxy, String> {
    BrowserNetworkProxy::start(origins, forbidden_ports).await
}

impl BrowserNetworkProxy {
    pub async fn start(origins: Vec<String>, forbidden_ports: Vec<u16>) -> Result<Self, String> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|_| "Browser network boundary could not start")?;
        let port = listener
            .local_addr()
            .map_err(|_| "Browser proxy address unavailable")?
            .port();
        let mut policy = Policy::default();
        policy.forbidden_ports.extend([1420, 9420, port]);
        policy.forbidden_ports.extend(forbidden_ports);
        for origin in origins {
            let url = checked_url(&origin)?;
            if !policy
                .forbidden_ports
                .contains(&url.port_or_known_default().unwrap_or(0))
            {
                policy.local.insert(url.origin().ascii_serialization());
            }
        }
        let policy = Arc::new(RwLock::new(policy));
        let (stop, mut stop_rx) = watch::channel(false);
        let (changed, changed_rx) = watch::channel(0);
        let context = Context {
            policy: policy.clone(),
            stop: stop_rx.clone(),
            changed: changed_rx,
        };
        let slots = Arc::new(Semaphore::new(24));
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = stop_rx.changed() => break,
                    accepted = listener.accept() => {
                        let Ok((socket, _)) = accepted else { break };
                        let Ok(slot) = slots.clone().try_acquire_owned() else { drop(socket); continue };
                        let context = context.clone();
                        tokio::spawn(async move {
                            let _slot = slot;
                            let _ = timeout(REQUEST_TIMEOUT, serve(socket, context)).await;
                        });
                    }
                }
            }
        });
        Ok(Self {
            port,
            policy,
            stop,
            changed,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn browser_args(&self) -> String {
        format!("--proxy-server=http://127.0.0.1:{} --proxy-bypass-list=<-loopback> --disable-quic --force-webrtc-ip-handling-policy=disable_non_proxied_udp --disable-background-networking", self.port)
    }

    pub fn allows(&self, raw: &str) -> bool {
        self.permits_navigation(raw)
    }

    /// Synchronous preflight only. The proxy additionally validates resolved IPs.
    pub fn permits_navigation(&self, raw: &str) -> bool {
        let Ok(url) = checked_url(raw) else {
            return false;
        };
        self.policy
            .read()
            .map(|p| permitted(&p, &url))
            .unwrap_or(false)
    }

    /// Called only by the trusted host after explicit user navigation or a
    /// Gateway-approved operation. Website content cannot mutate this policy.
    pub fn authorize_local_origin(&self, raw: &str) -> Result<(), String> {
        let url = checked_url(raw)?;
        let mut policy = self
            .policy
            .write()
            .map_err(|_| "Browser policy unavailable")?;
        if policy
            .forbidden_ports
            .contains(&url.port_or_known_default().unwrap_or(0))
        {
            return Err("Assistant control ports cannot be browsed".into());
        }
        if !explicit_local_host(url.host_str().unwrap_or_default()) {
            return Err("Local allowance requires localhost or a literal IP address".into());
        }
        policy.local.insert(url.origin().ascii_serialization());
        policy.revision = policy.revision.wrapping_add(1);
        let revision = policy.revision;
        drop(policy);
        let _ = self.changed.send(revision);
        Ok(())
    }

    pub fn revoke_local_origin(&self, raw: &str) -> Result<(), String> {
        let url = checked_url(raw)?;
        let mut policy = self
            .policy
            .write()
            .map_err(|_| "Browser policy unavailable")?;
        policy.local.remove(&url.origin().ascii_serialization());
        policy.revision = policy.revision.wrapping_add(1);
        let revision = policy.revision;
        drop(policy);
        let _ = self.changed.send(revision);
        Ok(())
    }

    pub fn shutdown(&self) {
        let _ = self.stop.send(true);
    }
}
impl Drop for BrowserNetworkProxy {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Clone)]
struct Context {
    policy: Arc<RwLock<Policy>>,
    stop: watch::Receiver<bool>,
    changed: watch::Receiver<u64>,
}

#[cfg(test)]
#[path = "network/tests.rs"]
mod tests;
