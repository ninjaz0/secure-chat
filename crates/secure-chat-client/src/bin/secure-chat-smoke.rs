#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let report = if let Ok(relay_url) = std::env::var("SECURE_CHAT_SMOKE_RELAY_URL") {
        secure_chat_client::run_relay_smoke_against(&relay_url).await?
    } else {
        secure_chat_client::run_relay_smoke().await?
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
