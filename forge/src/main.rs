use forge::core::{agent::CodingAgent, indexer::Indexer};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let index_path = Path::new(".index");
    let project_root = Path::new(".");

    // Create and initialize the indexer
    let indexer = Indexer::new(index_path)?;
    indexer.index_project(project_root)?;

    // Create the coding agent
    let agent = CodingAgent { indexer };

    // Perform a search query
    let query_str = "example";
    let limit = 10;
    let results = agent.search(query_str, limit)?;

    // Print the search results
    for (path, content) in results {
        println!("Path: {}", path);
        println!("Content: {}", content);
    }

    Ok(())
}
