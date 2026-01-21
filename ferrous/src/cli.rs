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
    println!("{}", "Available commands:".bold().bright_cyan());
    println!("  exit/quit - Exit the program");
    println!("  clear     - Clear conversation history");
    println!("  help      - Show this help message");
    println!("\n{}", "CLI Options:".bold().bright_cyan());
    println!("  --model <PATH>        Path to GGUF model");
    println!("  --port <PORT>         Server port (default: 8080)");
    println!("  --temperature <0-2>   Sampling temperature (default: 0.4)");
    println!("  --top-p <0-1>         Nucleus sampling (default: 0.9)");
    println!("  --top-k <int>         Top-K sampling (default: 50)");
    println!("  --max-tokens <int>    Max output tokens (default: 2048)");
    println!("  query --text <QUERY>  Run a single query");
}
