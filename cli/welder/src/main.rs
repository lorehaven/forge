#[tokio::main]
async fn main() -> quench_core::Result<()> {
    welder::run::run().await
}
