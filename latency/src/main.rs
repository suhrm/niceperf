
use anyhow::Result;
use common::QuicClient;
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let server_addr = "127.0.0.1:4443";
    let mut client = QuicClient::new(None, None, "")?;
    client.connect(server_addr.parse()?).await?;

    Ok(())
}
