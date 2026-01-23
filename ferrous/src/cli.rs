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
    println!("  --debug                     Show llama-server logs and more verbose output");
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
    println!("  read_file(path)");
    println!("      → Reads and returns the full content of a file");
    println!();
    println!("  write_file(path, content)");
    println!("      → Creates parent directories if needed and writes/overwrites the file");
    println!();
    println!("  list_directory([path])");
    println!("      → Lists files and immediate subdirectories (default: current dir \".\")");
    println!();
    println!("  get_directory_tree([path])");
    println!("      → Shows recursive directory structure (like tree command, default: \".\")");
    println!();
    println!("  create_directory(path)");
    println!("      → Creates a directory and all necessary parent directories");
    println!();
    println!("  file_exists(path)");
    println!("      → Returns \"true\" if file or directory exists, \"false\" otherwise");
    println!();
    println!("  replace_in_file(path, search, replace)");
    println!("      → Replaces all exact occurrences of <search> with <replace> in the file");
    println!();
    println!("  git_status                  Show current git status (short format)");
    println!("  git_diff([path])            Show changes (whole repo or specific path)");
    println!("  git_add(path)               Stage file/directory (or \".\" for all)");
    println!("  git_commit(message)         Create commit with given message");

    println!("{}", "Quick tips:".bright_yellow().bold());
    println!("  • Use 'clear' when the model starts repeating itself or forgetting recent changes");
    println!("  • --debug helps a lot when debugging tool calls or prompt issues");
    println!("  • Lower temperature (0.1–0.4) usually gives more precise code edits");
    println!("  • Models with long context (≥16k) work noticeably better for projects");
    println!();
}
