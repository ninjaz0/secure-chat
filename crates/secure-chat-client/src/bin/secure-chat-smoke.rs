#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::var("SECURE_CHAT_SMOKE_MODE").unwrap_or_default();
    let value = if mode == "group" {
        if let Ok(relay_url) = std::env::var("SECURE_CHAT_SMOKE_RELAY_URL") {
            serde_json::to_value(secure_chat_client::run_group_smoke_against(&relay_url).await?)?
        } else {
            serde_json::to_value(secure_chat_client::run_group_smoke().await?)?
        }
    } else if mode == "p2p" {
        if let (Ok(relay_url), Ok(p2p_addr)) = (
            std::env::var("SECURE_CHAT_SMOKE_RELAY_URL"),
            std::env::var("SECURE_CHAT_P2P_RENDEZVOUS_ADDR"),
        ) {
            serde_json::to_value(
                secure_chat_client::run_p2p_smoke_against(&relay_url, p2p_addr.parse()?).await?,
            )?
        } else {
            serde_json::to_value(secure_chat_client::run_p2p_smoke().await?)?
        }
    } else if let Ok(relay_url) = std::env::var("SECURE_CHAT_SMOKE_RELAY_URL") {
        serde_json::to_value(secure_chat_client::run_relay_smoke_against(&relay_url).await?)?
    } else {
        serde_json::to_value(secure_chat_client::run_relay_smoke().await?)?
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
