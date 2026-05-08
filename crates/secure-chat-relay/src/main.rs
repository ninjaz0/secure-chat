use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "secure_chat_relay=info,tower_http=info".into()),
        )
        .init();

    let http_addr: SocketAddr = std::env::var("SECURE_CHAT_RELAY_HTTP_ADDR")
        .or_else(|_| std::env::var("SECURE_CHAT_RELAY_ADDR"))
        .unwrap_or_else(|_| "127.0.0.1:8787".to_string())
        .parse()
        .expect("valid SECURE_CHAT_RELAY_HTTP_ADDR");
    let https_addr = std::env::var("SECURE_CHAT_RELAY_HTTPS_ADDR")
        .ok()
        .map(|value| {
            value
                .parse::<SocketAddr>()
                .expect("valid SECURE_CHAT_RELAY_HTTPS_ADDR")
        });
    let quic_addr = std::env::var("SECURE_CHAT_RELAY_QUIC_ADDR")
        .ok()
        .map(|value| {
            value
                .parse::<SocketAddr>()
                .expect("valid SECURE_CHAT_RELAY_QUIC_ADDR")
        });
    let cert = std::env::var("SECURE_CHAT_TLS_CERT").ok();
    let key = std::env::var("SECURE_CHAT_TLS_KEY").ok();
    let state = match std::env::var("SECURE_CHAT_RELAY_DB") {
        Ok(path) => {
            secure_chat_relay::AppState::persistent(path).expect("open SECURE_CHAT_RELAY_DB")
        }
        Err(_) => secure_chat_relay::AppState::memory(),
    };

    match (https_addr, quic_addr, cert, key) {
        (None, None, _, _) => {
            secure_chat_relay::run_http_with_state(http_addr, state)
                .await
                .expect("serve relay");
        }
        (https_addr, quic_addr, Some(cert), Some(key)) => {
            let http_state = state.clone();
            let http = tokio::spawn(async move {
                secure_chat_relay::run_http_with_state(http_addr, http_state)
                    .await
                    .map_err(|err| err.to_string())
            });
            let https = https_addr.map(|addr| {
                let cert = cert.clone();
                let key = key.clone();
                let state = state.clone();
                tokio::spawn(async move {
                    secure_chat_relay::run_https_with_state(addr, cert, key, state)
                        .await
                        .map_err(|err| err.to_string())
                })
            });
            let quic = quic_addr.map(|addr| {
                let cert = cert.clone();
                let key = key.clone();
                let state = state.clone();
                tokio::spawn(async move {
                    secure_chat_relay::run_quic_with_state(addr, cert, key, state)
                        .await
                        .map_err(|err| err.to_string())
                })
            });

            tokio::select! {
                result = http => panic!("HTTP relay exited: {:?}", result),
                result = async {
                    if let Some(https) = https { https.await } else { std::future::pending().await }
                } => panic!("HTTPS relay exited: {:?}", result),
                result = async {
                    if let Some(quic) = quic { quic.await } else { std::future::pending().await }
                } => panic!("QUIC relay exited: {:?}", result),
            }
        }
        _ => {
            panic!("SECURE_CHAT_TLS_CERT and SECURE_CHAT_TLS_KEY are required for HTTPS/QUIC");
        }
    }
}
