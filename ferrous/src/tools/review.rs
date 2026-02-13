use super::utils::{clean_path, resolve_dir};
use anyhow::{Result, anyhow};
use serde_json::Value;
use std::path::Path;
use walkdir::WalkDir;

const LONG_LINE_THRESHOLD: usize = 120;
const LONG_FUNCTION_THRESHOLD: usize = 50;

pub fn review_code(cwd: &Path, args: &Value) -> Result<String> {
    let raw_path: String = serde_json::from_value(args["path"].clone())?;
    let path = clean_path(&raw_path);
    let full = resolve_dir(cwd, &path)?;

    if !full.is_file() {
        return Err(anyhow!("Path is not a file: {path}"));
    }

    let content = std::fs::read_to_string(&full)?;
    let lang = detect_language(&path);

    let line_count = content.lines().count();
    let avg_line_length = if line_count > 0 {
        content.lines().map(str::len).sum::<usize>() / line_count
    } else {
        0
    };

    let mut warnings: Vec<String> = Vec::new();
    let mut suggestions: Vec<String> = Vec::new();

    warnings.extend(find_long_lines(&content));

    if matches!(
        lang.as_str(),
        "rust" | "javascript" | "typescript" | "python"
    ) {
        warnings.extend(find_long_functions(&content, &lang));
    }

    let todo_count = content
        .lines()
        .filter(|line| line.to_lowercase().contains("todo"))
        .count();
    if todo_count > 0 {
        suggestions.push(format!(
            "Found {todo_count} TODO comments - consider addressing them"
        ));
    }

    let commented_lines = count_commented_code(&content, &lang);
    if commented_lines > 5 {
        suggestions.push(format!(
            "Found {commented_lines} lines of commented code - consider removing dead code"
        ));
    }

    if matches!(lang.as_str(), "rust" | "javascript" | "typescript") {
        suggestions.extend(find_magic_number_suggestions(&content));
    }

    let mut output = Vec::new();
    output.push(format!("=== Code Review: {path} ==="));
    output.push(format!("Language: {lang}"));
    output.push(format!("Lines: {line_count}"));
    output.push(format!("Average line length: {avg_line_length}"));
    output.push(String::new());

    if !warnings.is_empty() {
        output.push("WARNINGS:".to_string());
        output.extend(warnings.iter().take(10).map(|item| format!("  - {item}")));
        if warnings.len() > 10 {
            output.push(format!("  ... and {} more warnings", warnings.len() - 10));
        }
        output.push(String::new());
    }

    if !suggestions.is_empty() {
        output.push("SUGGESTIONS:".to_string());
        output.extend(suggestions.iter().map(|item| format!("  - {item}")));
        output.push(String::new());
    }

    if warnings.is_empty() && suggestions.is_empty() {
        output.push("No major issues found.".to_string());
        output.push("Consider running a linter for deeper analysis.".to_string());
    }

    Ok(output.join("\n"))
}

pub fn suggest_refactorings(cwd: &Path, args: &Value) -> Result<String> {
    let raw_path: String = serde_json::from_value(args["path"].clone())?;
    let path = clean_path(&raw_path);
    let full = resolve_dir(cwd, &path)?;

    if !full.is_file() {
        return Err(anyhow!("Path is not a file: {path}"));
    }

    let content = std::fs::read_to_string(&full)?;
    let lines: Vec<&str> = content.lines().collect();

    let mut output = Vec::new();
    output.push(format!("=== Refactoring Suggestions: {path} ==="));
    output.push(String::new());

    if has_duplication(&lines, &content) {
        output.push("Code duplication detected. Consider extracting shared logic.".to_string());
    }

    if let Some((line_no, params)) = find_long_parameter_list(&lines) {
        output.push(format!(
            "Long parameter list at line {line_no}: {params} parameters"
        ));
    }

    let max_indent = lines
        .iter()
        .map(|line| line.chars().take_while(|ch| ch.is_whitespace()).count())
        .max()
        .unwrap_or(0);
    if max_indent > 20 {
        output.push(format!("Deep nesting detected (max indent: {max_indent})"));
    }

    output.push("Run tests after each refactor step to prevent regressions.".to_string());

    Ok(output.join("\n"))
}

pub fn review_module(cwd: &Path, args: &Value) -> Result<String> {
    let raw_path = args["path"].as_str().unwrap_or(".");
    let path = clean_path(raw_path);
    let full = resolve_dir(cwd, &path)?;

    if !full.is_dir() {
        return Err(anyhow!("Path is not a directory: {path}"));
    }

    let mut files = collect_reviewable_files(cwd, &full);
    files.sort();

    if files.is_empty() {
        return Ok(format!("No reviewable source files found in '{path}'."));
    }

    let mut total_warnings = 0_usize;
    let mut total_todos = 0_usize;
    let mut hotspots: Vec<(usize, String, usize, usize)> = Vec::new();

    for rel_path in &files {
        let abs = resolve_dir(cwd, rel_path)?;
        let Ok(content) = std::fs::read_to_string(&abs) else {
            continue;
        };

        let warning_count = content
            .lines()
            .filter(|line| line.len() > LONG_LINE_THRESHOLD)
            .count();
        let todo_count = content
            .lines()
            .filter(|line| line.to_lowercase().contains("todo"))
            .count();

        total_warnings += warning_count;
        total_todos += todo_count;

        let score = warning_count.saturating_mul(2) + todo_count;
        if score > 0 {
            hotspots.push((score, rel_path.clone(), warning_count, todo_count));
        }
    }

    let mut output = Vec::new();
    output.push(format!("=== Module Review: {path} ==="));
    output.push(format!("Files reviewed: {}", files.len()));
    output.push(format!("Aggregate long-line warnings: {total_warnings}"));
    output.push(format!("Aggregate TODO markers: {total_todos}"));
    output.push(String::new());

    if hotspots.is_empty() {
        output.push("No major module-wide heuristics triggered.".to_string());
        output.push("Use review_code on specific files for deeper analysis.".to_string());
        return Ok(output.join("\n"));
    }

    hotspots.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    output.push("Top files to inspect with review_code next:".to_string());
    output.extend(hotspots.iter().take(10).map(|(_, file, warnings, todos)| {
        format!("  - {file} (long lines: {warnings}, TODOs: {todos})")
    }));

    Ok(output.join("\n"))
}

fn detect_language(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map_or("unknown", |ext| match ext {
            "rs" => "rust",
            "js" | "jsx" => "javascript",
            "ts" | "tsx" => "typescript",
            "py" => "python",
            "go" => "go",
            "c" => "c",
            "cpp" | "cc" | "cxx" => "c++",
            "java" => "java",
            "rb" => "ruby",
            "php" => "php",
            "sh" | "bash" => "shell",
            "md" => "markdown",
            "toml" => "toml",
            "yaml" | "yml" => "yaml",
            "json" => "json",
            _ => ext,
        })
        .to_string()
}

fn find_long_lines(content: &str) -> Vec<String> {
    let mut warnings = Vec::new();

    for (index, line) in content.lines().enumerate() {
        if line.len() <= LONG_LINE_THRESHOLD {
            continue;
        }

        let trimmed = line.trim();
        let comment_content = trimmed
            .strip_prefix("//")
            .or_else(|| trimmed.strip_prefix("/*"))
            .or_else(|| trimmed.strip_prefix('*'))
            .unwrap_or(trimmed);

        let decorative = comment_content
            .chars()
            .filter(|ch| {
                !matches!(
                    ch,
                    '─' | '━'
                        | '│'
                        | '┃'
                        | '┌'
                        | '┐'
                        | '└'
                        | '┘'
                        | '├'
                        | '┤'
                        | '┬'
                        | '┴'
                        | '┼'
                        | '-'
                        | '='
                        | '_'
                        | ' '
                )
            })
            .count()
            < 10;

        if decorative {
            continue;
        }

        let preview = if line.chars().count() > 80 {
            let truncated: String = line.chars().take(77).collect();
            format!("{truncated}...")
        } else {
            line.to_string()
        };

        warnings.push(format!(
            "Line {}: Long line ({} chars)\\n      {preview}",
            index + 1,
            line.len()
        ));
    }

    warnings
}

fn find_long_functions(content: &str, lang: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut in_function = false;
    let mut function_start = 0_usize;
    let mut brace_depth = 0_usize;
    let mut function_name = String::new();

    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if !in_function && is_function_start(trimmed, lang) {
            in_function = true;
            function_start = index + 1;
            function_name = extract_function_name(trimmed, lang);
            brace_depth = 0;
        }

        if !in_function {
            continue;
        }

        let opens = trimmed.chars().filter(|&ch| ch == '{').count();
        let closes = trimmed.chars().filter(|&ch| ch == '}').count();
        brace_depth = brace_depth.saturating_add(opens).saturating_sub(closes);

        if brace_depth == 0 && trimmed.contains('}') {
            let function_len = index + 1 - function_start;
            if function_len > LONG_FUNCTION_THRESHOLD {
                warnings.push(format!(
                    "Function '{function_name}' at line {function_start} is {function_len} lines long"
                ));
            }
            in_function = false;
        }
    }

    warnings
}

fn is_function_start(trimmed: &str, lang: &str) -> bool {
    match lang {
        "rust" => trimmed.starts_with("fn ") || trimmed.starts_with("pub fn "),
        "javascript" | "typescript" => trimmed.contains("function "),
        "python" => trimmed.starts_with("def "),
        _ => false,
    }
}

fn extract_function_name(line: &str, lang: &str) -> String {
    let parts: Vec<&str> = line.split_whitespace().collect();
    match lang {
        "rust" => extract_word_after(&parts, "fn"),
        "javascript" | "typescript" => extract_word_after(&parts, "function"),
        "python" => extract_word_after(&parts, "def"),
        _ => "unknown".to_string(),
    }
}

fn extract_word_after(parts: &[&str], needle: &str) -> String {
    parts
        .iter()
        .enumerate()
        .find(|(_, part)| **part == needle)
        .and_then(|(index, _)| parts.get(index + 1).copied())
        .map_or_else(
            || "unknown".to_string(),
            |word| word.trim_end_matches('(').to_string(),
        )
}

fn count_commented_code(content: &str, lang: &str) -> usize {
    let mut count = 0_usize;

    for line in content.lines() {
        let trimmed = line.trim();
        if matches!(
            lang,
            "rust" | "javascript" | "typescript" | "c" | "c++" | "java"
        ) {
            if trimmed.starts_with("//")
                && !trimmed.starts_with("///")
                && (trimmed.contains(';')
                    || trimmed.contains('{')
                    || trimmed.contains('}')
                    || trimmed.contains("fn ")
                    || trimmed.contains("let ")
                    || trimmed.contains("const "))
            {
                count += 1;
            }
        } else if matches!(lang, "python" | "shell")
            && trimmed.starts_with('#')
            && !trimmed.starts_with("#!/")
            && (trimmed.contains(':')
                || trimmed.contains('=')
                || trimmed.contains("def ")
                || trimmed.contains("class "))
        {
            count += 1;
        }
    }

    count
}

fn find_magic_number_suggestions(content: &str) -> Vec<String> {
    let mut locations: Vec<(usize, String, String)> = Vec::new();

    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with('"') {
            continue;
        }

        for word in trimmed.split(|ch: char| !ch.is_numeric() && ch != '-') {
            if word.is_empty() || word == "-" {
                continue;
            }

            let Ok(num) = word.parse::<i64>() else {
                continue;
            };

            if matches!(num, -1..=2) {
                continue;
            }

            let preview = if trimmed.chars().count() > 60 {
                let truncated: String = trimmed.chars().take(57).collect();
                format!("{truncated}...")
            } else {
                trimmed.to_string()
            };
            locations.push((index + 1, word.to_string(), preview));
        }
    }

    if locations.is_empty() {
        return Vec::new();
    }

    let mut suggestions = Vec::new();
    suggestions.push(format!(
        "Consider extracting {} magic numbers into named constants:",
        locations.len()
    ));
    suggestions.extend(
        locations
            .iter()
            .take(5)
            .map(|(line_no, number, preview)| format!("  Line {line_no}: {number} in '{preview}'")),
    );
    if locations.len() > 5 {
        suggestions.push(format!("  ... and {} more", locations.len() - 5));
    }

    suggestions
}

fn has_duplication(lines: &[&str], full_content: &str) -> bool {
    let mut repeated_chunks = 0_usize;
    for index in 0..lines.len().saturating_sub(5) {
        let chunk = lines[index..index + 5].join("\n");
        if full_content.matches(&chunk).count() > 1 {
            repeated_chunks += 1;
        }
        if repeated_chunks > 3 {
            return true;
        }
    }
    false
}

fn find_long_parameter_list(lines: &[&str]) -> Option<(usize, usize)> {
    lines.iter().enumerate().find_map(|(index, line)| {
        if !(line.contains("fn ") || line.contains("function ")) {
            return None;
        }
        let params = line.matches(',').count() + 1;
        (params > 4).then_some((index + 1, params))
    })
}

fn collect_reviewable_files(cwd: &Path, root: &Path) -> Vec<String> {
    let allowed_exts = [
        "rs", "js", "jsx", "ts", "tsx", "py", "go", "c", "cc", "cpp", "cxx", "h", "hpp", "java",
        "rb", "php", "sh", "bash", "toml", "yaml", "yml", "json", "md",
    ];

    let base = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let mut files = Vec::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            !name.starts_with('.') && ![".git", "target", "node_modules"].contains(&&*name)
        })
        .filter_map(std::result::Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }

        let ext = entry
            .path()
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if !allowed_exts.contains(&ext) {
            continue;
        }

        let rel = entry
            .path()
            .strip_prefix(&base)
            .unwrap_or_else(|_| entry.path());
        files.push(rel.to_string_lossy().to_string());
    }

    files
}
