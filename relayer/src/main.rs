use ws::WsServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::try_init_from_env("WEBB_LOG")?;
    let mut server = WsServer::new("0.0.0.0:9933").await?;
    server.register_method("webb_Relay", |_| Ok("Todo!"))?;
    println!("Starting on port 9933");
    server.start().await;
    Ok(())
}
