#[tokio::main]
async fn main() -> anyhow::Result<()> {
    welder::run::run().await
}
