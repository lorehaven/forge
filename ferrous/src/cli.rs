use crate::llm::ModelLoadPhase;
use crate::plan::ExecutionPlan;
use colored::Colorize;
use once_cell::sync::Lazy;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

pub fn syntax_for_lang(lang: &str) -> &SyntaxReference {
    SYNTAX_SET
        .find_syntax_by_name(lang)
        .or_else(|| SYNTAX_SET.find_syntax_by_extension(lang))
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text())
}

fn highlight_code_block(code: &str, lang: &str) -> String {
    let theme = &THEME_SET.themes["base16-ocean.dark"];
    let syntax = syntax_for_lang(lang);
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut output = String::new();
    for line in LinesWithEndings::from(code) {
        let regions = highlighter.highlight_line(line, &SYNTAX_SET).unwrap();
        let styled = syntect::util::as_24_bit_terminal_escaped(&regions, false);
        output.push_str(&styled);
    }
    output.push_str("\x1b[0m");
    output
}

pub fn pretty_print_response(response: &str) {
    let mut in_code = false;
    let mut current_lang = "text";
    let mut code_buffer = String::new();

    for line in response.lines() {
        if line.starts_with("```") {
            if in_code {
                let highlighted = highlight_code_block(&code_buffer, current_lang);
                println!("{}", highlighted);
                code_buffer.clear();
                in_code = false;
            } else {
                let lang = line.strip_prefix("```").unwrap_or("").trim();
                current_lang = if lang.is_empty() { "text" } else { lang };
                in_code = true;
            }
            continue;
        }

        if in_code {
            code_buffer.push_str(line);
            code_buffer.push('\n');
        } else {
            let styled = if line.starts_with('#') {
                line.bold().bright_blue().to_string()
            } else if line.starts_with("- ") || line.starts_with("* ") || line.starts_with("• ") {
                line.bright_green().to_string()
            } else {
                line.normal().to_string()
            };
            println!("{}", styled);
        }
    }

    if in_code {
        let highlighted = highlight_code_block(&code_buffer, current_lang);
        println!("{}", highlighted);
    }
}

pub fn render_plan(plan: &ExecutionPlan) {
    println!("\n{}", "────── Plan ──────".bright_black());
    println!("{}", plan);
    println!("{}", "─────────────────".dimmed());
}

pub fn render_model_progress(phase: ModelLoadPhase) {
    match phase {
        ModelLoadPhase::StartingServer => {
            println!("{}", "Starting model server…".bright_blue());
        }
        ModelLoadPhase::WaitingForPort => {
            println!("{}", "Loading model (waiting for server)…".bright_yellow());
        }
        ModelLoadPhase::Ready => {
            println!("{}", "Model ready.".bright_green().bold());
        }
    }
}

pub fn print_indented(text: &str) {
    for line in text.lines() {
        println!("│ {}", line);
    }
}

pub fn print_help() {
    println!(
        "{}",
        "Ferrous – fast, local coding assistant"
            .bright_cyan()
            .bold()
    );
    println!();

    println!(
        "{}",
        "REPL commands (type these at the >> prompt):"
            .bright_yellow()
            .bold()
    );
    println!("  exit / quit       Exit the program");
    println!("  clear             Reset conversation history (keeps system prompt)");
    println!("  save [name]       Save current conversation (auto id + timestamp)");
    println!("                    Example: save \"feature x planning\"");
    println!("  load <prefix>     Load by name prefix or short uuid (first 8 chars)");
    println!("  list              List all saved conversations");
    println!("  delete <prefix>   Delete conversation by name prefix or short uuid");
    println!("  config / cfg      Show current configuration (merged values)");
    println!("  help              Show this help message");
    println!();

    println!(
        "{}",
        "Important CLI flags (set when launching ferrous):"
            .bright_yellow()
            .bold()
    );
    println!("  --model <PATH>              Path to GGUF model file");
    println!("  --port <PORT>               llama.cpp server port (default: 8080)");
    println!("  --temperature <0.0–2.0>     Sampling temperature (default: 0.4)");
    println!("  --top-p <0.0–1.0>           Nucleus sampling probability (default: 0.9)");
    println!("  --top-k <int>               Top-K sampling (default: 50)");
    println!("  --max-tokens <int>          Max output tokens (default: 8192)");
    println!("  --debug                     Show llama-server logs and verbose output");
    println!("  --mirostat <0-2>            Mirostat mode (0=off, 1=v1, 2=v2) (default: 0)");
    println!("  --mirostat-tau <float>      Target surprise for Mirostat (default: 5.0)");
    println!("  --mirostat-eta <float>      Adaptation rate for Mirostat (default: 0.1)");
    println!();

    println!("{}", "One-shot mode example:".bright_yellow().bold());
    println!("  ferrous query --text \"explain this function in main.rs\" --temperature 0.2");
    println!();

    println!(
        "{}",
        "Tools the agent can use (model decides when to call them):"
            .bright_yellow()
            .bold()
    );

    println!("  analyze_project()");
    println!("      → Run cargo clippy for project-wide linting and analysis");
    println!();

    println!("  get_file_info(path)");
    println!("      → Metadata for file or directory (size, mtime, type, line count if text)");
    println!();

    println!("  file_exists(path)");
    println!("      → Returns \"true\" if file or directory exists");
    println!();

    println!("  read_file(path)");
    println!("      → Reads and returns full file contents");
    println!();

    println!("  read_multiple_files(paths)");
    println!("      → Read contents of multiple files at once (array of paths)");
    println!();

    println!("  write_file(path, content)");
    println!("      → Writes or overwrites file, creates parent directories");
    println!();

    println!("  append_to_file(path, content)");
    println!("      → Appends content to file, creates file if missing");
    println!();

    println!("  replace_in_file(path, search, replace)");
    println!("      → Replaces all exact matches of <search> with <replace>");
    println!();

    println!("  create_directory(path)");
    println!("      → Creates directory and parents (idempotent)");
    println!();

    println!("  list_directory([path])");
    println!("      → Lists files and directories (non-recursive, default: \".\")");
    println!();

    println!("  get_directory_tree([path])");
    println!("      → Recursive directory tree (default: \".\")");
    println!();

    println!("  list_files_recursive([path], [extension])");
    println!("      → Flat list of all regular files, optional extension filter");
    println!();

    println!("  search_text(pattern, [path], [case_sensitive])");
    println!("      → Grep-like search for lines containing pattern");
    println!();

    println!("  execute_shell_command(command)");
    println!("      → Execute allowed shell command (cargo only)");
    println!();

    println!("  git_status");
    println!("      → Show git status (short)");
    println!();

    println!("  git_diff([path])");
    println!("      → Show git diff (repo or specific path)");
    println!();

    println!("  git_add(path)");
    println!("      → Stage file or directory for commit");
    println!();

    println!("  git_commit(message)");
    println!("      → Create git commit with message");
    println!();

    println!("{}", "Config File:".bright_yellow().bold());
    println!("  • config.toml in current directory for project-specific defaults");
    println!("  • Example:");
    println!("      model = \"path/to/my-model.gguf\"");
    println!("      temperature = 0.2");
    println!("      debug = true");
    println!("      mirostat = 2");
    println!("      mirostat_tau = 5.0");
    println!("      mirostat_eta = 0.1");
    println!();

    println!("{}", "Quick tips:".bright_yellow().bold());
    println!("  • Use 'clear' when the model starts repeating or losing context");
    println!("  • --debug is useful for diagnosing tool calls and prompts");
    println!("  • Lower temperature (0.1–0.4) yields more precise code edits");
    println!("  • Enable Mirostat (e.g., 2) for more natural, adaptive sampling on long responses");
    println!("  • Long-context models (≥16k) perform better on multi-file projects");
    println!();
}
