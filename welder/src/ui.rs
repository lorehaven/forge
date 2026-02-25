// ─────────────────────────────────────────────────────────────
// Shared UI — colours, separators, and display helpers used by
// the backend banner, REPL, and agent executor alike.
// ─────────────────────────────────────────────────────────────

pub const RESET:  &str = "\x1b[0m";
pub const BOLD:   &str = "\x1b[1m";
pub const DIM:    &str = "\x1b[2m";
pub const CYAN:   &str = "\x1b[36m";
pub const BLUE:   &str = "\x1b[34m";
pub const GREEN:  &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const WHITE:  &str = "\x1b[37m";

pub const SEP:      &str = "────────────────────────────────────────────────────────";
pub const SEP_THIN: &str = "┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄";

/// Print the backend-ready banner.
pub fn print_backend_banner(kind: &str, url: &str) {
    println!(
        "\n{CYAN}{SEP}{RESET}\n\
         {BOLD}{WHITE}  ▸ LLM BACKEND READY{RESET}\n\
         {CYAN}{SEP}{RESET}\n\
         {DIM}  type  {RESET}{WHITE}{kind}{RESET}\n\
         {DIM}  url   {RESET}{WHITE}{url}{RESET}\n\
         {CYAN}{SEP}{RESET}\n"
    );
}

/// Print the workflow header and agent graph.
pub fn print_workflow_header<S: std::hash::BuildHasher>(
    workflow_path: &str,
    root_name: &str,
    agents: &std::collections::HashMap<String, crate::engine::executor::AgentNode, S>,
) {
    println!(
        "{CYAN}{SEP}{RESET}\n\
         {BOLD}{WHITE}  ▸ WORKFLOW{RESET}  {DIM}{workflow_path}{RESET}\n\
         {CYAN}{SEP}{RESET}"
    );
    print_graph_node(root_name, agents, 0);
    println!("{CYAN}{SEP_THIN}{RESET}");
    println!("{DIM}  type 'exit' to quit{RESET}\n");
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
            format!("  {DIM}[{} tools]{RESET}", node.tools.len())
        };
        println!(
            "  {indent}{CYAN}{connector}{RESET} {BOLD}{WHITE}{name}{RESET}  \
             {DIM}{model_name}{RESET}{tools_hint}"
        );
        for child in &node.children {
            print_graph_node(child, agents, depth + 1);
        }
    } else {
        println!("  {indent}{CYAN}{connector}{RESET} {WHITE}{name}{RESET}");
    }
}

/// Print the final agent answer.
pub fn print_answer(text: &str) {
    println!("\n{CYAN}{SEP_THIN}{RESET}");
    println!("{GREEN}{BOLD}  ▸ Answer{RESET}");
    println!("{CYAN}{SEP_THIN}{RESET}");
    println!("{}", text.trim());
    println!("{CYAN}{SEP_THIN}{RESET}\n");
}

/// Print the REPL prompt (no newline).
pub fn print_prompt() {
    print!("{BOLD}{WHITE}You ▸{RESET} ");
}
