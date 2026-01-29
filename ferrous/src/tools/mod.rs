use anyhow::{Result, anyhow};
use serde_json::Value;

pub mod cargo;
pub mod file;
pub mod git;
pub mod shell;
pub mod utils;

pub async fn execute_tool(name: &str, args: Value) -> Result<String> {
    let cwd = std::env::current_dir()?;

    match name {
        "analyze_project" => cargo::analyze_project(&cwd),
        "get_file_info" => file::get_file_info(&cwd, &args),
        "file_exists" => file::file_exists(&cwd, &args),
        "list_files_recursive" => file::list_files_recursive(&cwd, &args),
        "replace_in_file" => file::replace_in_file(&cwd, &args),
        "read_file" => file::read_file(&cwd, &args),
        "read_multiple_files" => file::read_multiple_files(&cwd, &args),
        "write_file" => file::write_file(&cwd, &args),
        "list_directory" => file::list_directory(&cwd, &args),
        "get_directory_tree" => file::get_directory_tree(&cwd, &args),
        "create_directory" => file::create_directory(&cwd, &args),
        "append_to_file" => file::append_to_file(&cwd, &args),
        "search_text" => file::search_text(&cwd, &args),
        "execute_shell_command" => shell::execute_shell_command(&cwd, &args).await,
        "git_status" => git::git_status(&cwd),
        "git_diff" => git::git_diff(&cwd, &args),
        "git_add" => git::git_add(&cwd, &args),
        "git_commit" => git::git_commit(&cwd, &args),
        _ => Err(anyhow!("Unknown tool: {name}")),
    }
}
