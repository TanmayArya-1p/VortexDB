use std::net::SocketAddr;

use tokio::net::TcpListener;
use tracing;

#[inline(always)] // doing function inlining for optimization
pub fn log_rpc(rpc: &str, logging: bool) {
    if logging {
        tracing::event!(tracing::Level::INFO, "{} RPC called", rpc);
    }
}

// Either socket address or a premade listener
#[derive(Debug)]
pub enum ServerEndpoint {
    Address(SocketAddr),
    Listener(TcpListener),
}
