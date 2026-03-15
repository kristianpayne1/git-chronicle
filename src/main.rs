pub mod audit;
pub mod batcher;
pub mod cli;
pub mod error;
pub mod ingester;
pub mod llm;
pub mod reducer;
pub mod templates;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}