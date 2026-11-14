//! Terminal output, styled with forge-tui so welder looks like the other forge tools.

use forge_tui::console::{accent, muted, rule, section, strong};

/// Width of the rules between blocks.
const RULE_WIDTH: usize = 56;

/// Print the backend-ready banner.
pub fn print_backend_banner(kind: &str, url: &str) {
    println!(
        "\n{}\n{}\n{} {}\n{} {}\n{}\n",
        rule(RULE_WIDTH),
        section("LLM backend ready", ""),
        muted("  type "),
        strong(kind),
        muted("  url  "),
        strong(url),
        rule(RULE_WIDTH),
    );
}

/// Print the workflow header and agent graph.
pub fn print_workflow_header<S: std::hash::BuildHasher>(
    workflow_path: &str,
    root_name: &str,
    agents: &std::collections::HashMap<String, crate::engine::executor::AgentNode, S>,
) {
    println!(
        "{}\n{}\n{}",
        rule(RULE_WIDTH),
        section("Workflow", workflow_path),
        rule(RULE_WIDTH)
    );
    print_graph_node(root_name, agents, 0);
    println!("{}", rule(RULE_WIDTH));
    println!("{}\n", muted("  type 'exit' to quit"));
}

fn print_graph_node<S: std::hash::BuildHasher>(
    name: &str,
    agents: &std::collections::HashMap<String, crate::engine::executor::AgentNode, S>,
    depth: usize,
) {
    let indent = "  ".repeat(depth);
    let connector = if depth == 0 { "◆" } else { "└─" };

    if let Some(node) = agents.get(name) {
        let model_name = node.model.name();
        let tools_hint = if node.tools.is_empty() {
            String::new()
        } else {
            format!("  {}", muted(&format!("[{} tools]", node.tools.len())))
        };
        println!(
            "  {indent}{} {}  {}{tools_hint}",
            accent(connector),
            strong(name),
            muted(model_name),
        );
        for child in &node.children {
            print_graph_node(child, agents, depth + 1);
        }
    } else {
        println!("  {indent}{} {name}", accent(connector));
    }
}

/// Print the final agent answer.
pub fn print_answer(text: &str) {
    println!("\n{}", rule(RULE_WIDTH));
    println!("{}", section("Answer", ""));
    println!("{}", rule(RULE_WIDTH));
    println!("{}", text.trim());
    println!("{}\n", rule(RULE_WIDTH));
}

/// Print the REPL prompt (no newline).
pub fn print_prompt() {
    print!("{} ", strong("You ▸"));
}

/// The header of an agent's turn: a rule, its name with the model, and a rule.
pub fn print_agent_header(name: &str, model: Option<&str>, indent: &str) {
    println!(
        "\n{indent}{}\n{indent}{}\n{indent}{}",
        rule(RULE_WIDTH),
        section(name, model.unwrap_or("")),
        rule(RULE_WIDTH)
    );
}

/// A progress note under an agent's header (`routing…`, `executing…`).
pub fn print_note(indent: &str, text: &str) {
    println!("{indent}  {}", muted(text));
}

/// An agent handing the request to a child.
pub fn print_delegate(indent: &str, child: &str) {
    println!("{indent}  {}", accent(&format!("↳ {child}")));
}

/// An agent finished.
pub fn print_done(indent: &str, name: &str) {
    println!("{indent}  {}", accent(&format!("✓ {name}")));
}
