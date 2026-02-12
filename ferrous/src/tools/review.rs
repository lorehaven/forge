use anyhow::{Result, anyhow};
use serde_json::Value;
use std::path::Path;
use super::utils::{clean_path, resolve_dir};

/// Performs a code review on a file, analyzing quality, potential issues, and best practices
pub fn review_code(cwd: &Path, args: &Value) -> Result<String> {
    let raw_path: String = serde_json::from_value(args["path"].clone())?;
    let path = clean_path(&raw_path);
    let full = resolve_dir(cwd, &path)?;

    if !full.is_file() {
        return Err(anyhow!("Path is not a file: {}", path));
    }

    let content = std::fs::read_to_string(&full)?;

    // Detect language
    let lang = detect_language(&path);

    // Basic code analysis
    let line_count = content.lines().count();
    let avg_line_length = if line_count > 0 {
        content.lines().map(|l| l.len()).sum::<usize>() / line_count
    } else {
        0
    };

    let mut review = Vec::new();
    review.push(format!("=== Code Review: {} ===", path));
    review.push(format!("Language: {}", lang));
    review.push(format!("Lines: {}", line_count));
    review.push(format!("Average line length: {}", avg_line_length));
    review.push(String::new());

    // Check for common issues
    let issues: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut suggestions: Vec<String> = Vec::new();

    // Long lines - show preview of the line
    for (i, line) in content.lines().enumerate() {
        if line.len() > 120 {
            // Skip decorative comment lines (mostly box-drawing characters)
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
                let content_without_comment = if trimmed.starts_with("//") {
                    &trimmed[2..]
                } else if trimmed.starts_with("/*") {
                    &trimmed[2..]
                } else {
                    &trimmed[1..]
                };

                // If it's mostly box-drawing chars or dashes, skip it
                let non_decoration_chars = content_without_comment.chars()
                    .filter(|c| !matches!(c, '─' | '━' | '│' | '┃' | '┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼' | '-' | '=' | '_' | ' '))
                    .count();

                if non_decoration_chars < 10 {
                    continue; // Skip decorative lines
                }
            }

            let preview = if line.chars().count() > 80 {
                let truncated: String = line.chars().take(77).collect();
                format!("{}...", truncated)
            } else {
                line.to_string()
            };
            warnings.push(format!(
                "Line {}: Long line ({} chars)\n      {}",
                i + 1,
                line.len(),
                preview
            ));
        }
    }

    // Very long functions (heuristic)
    if lang == "rust" || lang == "javascript" || lang == "typescript" || lang == "python" {
        check_function_length(&content, &lang, &mut warnings);
    }

    // TODO comments
    let todo_count = content.lines().filter(|l| l.to_lowercase().contains("todo")).count();
    if todo_count > 0 {
        suggestions.push(format!("Found {} TODO comments - consider addressing them", todo_count));
    }

    // Commented out code
    let commented_lines = count_commented_code(&content, &lang);
    if commented_lines > 5 {
        suggestions.push(format!("Found {} lines of commented code - consider removing dead code", commented_lines));
    }

    // Magic numbers (numbers in code that aren't 0, 1, -1)
    if lang == "rust" || lang == "javascript" || lang == "typescript" {
        check_magic_numbers(&content, &mut suggestions);
    }

    // Compile results
    if !issues.is_empty() {
        review.push("🔴 ISSUES:".to_string());
        review.extend(issues.iter().map(|i| format!("  - {}", i)));
        review.push(String::new());
    }

    if !warnings.is_empty() {
        review.push("⚠️  WARNINGS:".to_string());
        review.extend(warnings.iter().take(10).map(|w| format!("  - {}", w)));
        if warnings.len() > 10 {
            review.push(format!("  ... and {} more warnings", warnings.len() - 10));
        }
        review.push(String::new());
    }

    if !suggestions.is_empty() {
        review.push("💡 SUGGESTIONS:".to_string());
        review.extend(suggestions.iter().map(|s| format!("  - {}", s)));
        review.push(String::new());
    }

    if issues.is_empty() && warnings.is_empty() && suggestions.is_empty() {
        review.push("✅ No major issues found!".to_string());
        review.push(String::new());
        review.push("Consider running a linter for more detailed analysis.".to_string());
    }

    Ok(review.join("\n"))
}

fn detect_language(path: &str) -> String {
    if let Some(ext) = Path::new(path).extension().and_then(|e| e.to_str()) {
        match ext {
            "rs" => "rust",
            "js" => "javascript",
            "jsx" => "javascript",
            "ts" => "typescript",
            "tsx" => "typescript",
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
        }
    } else {
        "unknown"
    }.to_string()
}

fn check_function_length(content: &str, lang: &str, warnings: &mut Vec<String>) {
    let mut in_function = false;
    let mut function_start = 0;
    let mut brace_count = 0;
    let mut function_name = String::new();

    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Simple heuristic for function detection
        if !in_function {
            if (lang == "rust" && (trimmed.starts_with("fn ") || trimmed.starts_with("pub fn "))) ||
               (lang == "javascript" && trimmed.contains("function ")) ||
               (lang == "typescript" && trimmed.contains("function ")) ||
               (lang == "python" && trimmed.starts_with("def ")) {
                in_function = true;
                function_start = i + 1;
                function_name = extract_function_name(trimmed, lang);
                brace_count = 0;
            }
        }

        if in_function {
            brace_count += trimmed.chars().filter(|&c| c == '{').count() as i32;
            brace_count -= trimmed.chars().filter(|&c| c == '}').count() as i32;

            if brace_count == 0 && trimmed.contains('}') {
                let length = i + 1 - function_start;
                if length > 50 {
                    warnings.push(format!("Function '{}' at line {} is {} lines long - consider breaking it up", function_name, function_start, length));
                }
                in_function = false;
            }
        }
    }
}

fn extract_function_name(line: &str, lang: &str) -> String {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if lang == "rust" {
        for (i, part) in parts.iter().enumerate() {
            if *part == "fn" && i + 1 < parts.len() {
                return parts[i + 1].trim_end_matches('(').to_string();
            }
        }
    } else if lang == "javascript" || lang == "typescript" {
        for (i, part) in parts.iter().enumerate() {
            if *part == "function" && i + 1 < parts.len() {
                return parts[i + 1].trim_end_matches('(').to_string();
            }
        }
    } else if lang == "python" {
        for (i, part) in parts.iter().enumerate() {
            if *part == "def" && i + 1 < parts.len() {
                return parts[i + 1].trim_end_matches('(').to_string();
            }
        }
    }
    "unknown".to_string()
}

fn count_commented_code(content: &str, lang: &str) -> usize {
    let mut count = 0;
    for line in content.lines() {
        let trimmed = line.trim();
        if lang == "rust" || lang == "javascript" || lang == "typescript" || lang == "c" || lang == "c++" || lang == "java" {
            if trimmed.starts_with("//") && !trimmed.starts_with("///") {
                // Check if it looks like code (contains common code patterns)
                if trimmed.contains(';') || trimmed.contains('{') || trimmed.contains('}') ||
                   trimmed.contains("fn ") || trimmed.contains("let ") || trimmed.contains("const ") {
                    count += 1;
                }
            }
        } else if lang == "python" || lang == "shell" {
            if trimmed.starts_with('#') && !trimmed.starts_with("#!/") {
                if trimmed.contains(':') || trimmed.contains('=') || trimmed.contains("def ") || trimmed.contains("class ") {
                    count += 1;
                }
            }
        }
    }
    count
}

fn check_magic_numbers(content: &str, suggestions: &mut Vec<String>) {
    let mut magic_number_locations: Vec<(usize, String, String)> = Vec::new(); // (line_num, number, line_preview)

    for (i, line) in content.lines().enumerate() {
        // Skip lines that are comments or strings
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with('"') {
            continue;
        }

        // Find numbers in the line (very basic regex-like matching)
        let words: Vec<&str> = trimmed.split(|c: char| !c.is_numeric() && c != '.' && c != '-').collect();
        for word in words {
            if let Ok(num) = word.parse::<f64>() {
                // Ignore common numbers
                if num != 0.0 && num != 1.0 && num != -1.0 && num != 2.0 && num.abs() > 0.0 {
                    let preview = if trimmed.chars().count() > 60 {
                        let truncated: String = trimmed.chars().take(57).collect();
                        format!("{}...", truncated)
                    } else {
                        trimmed.to_string()
                    };
                    magic_number_locations.push((i + 1, word.to_string(), preview));
                }
            }
        }
    }

    if !magic_number_locations.is_empty() {
        suggestions.push(format!("Consider extracting {} magic numbers into named constants:", magic_number_locations.len()));
        // Show first 5 occurrences
        for (line_num, number, preview) in magic_number_locations.iter().take(5) {
            suggestions.push(format!("  Line {}: {} in '{}'", line_num, number, preview));
        }
        if magic_number_locations.len() > 5 {
            suggestions.push(format!("  ... and {} more", magic_number_locations.len() - 5));
        }
    }
}

/// Suggests refactorings for a file based on common patterns
pub fn suggest_refactorings(cwd: &Path, args: &Value) -> Result<String> {
    let raw_path: String = serde_json::from_value(args["path"].clone())?;
    let path = clean_path(&raw_path);
    let full = resolve_dir(cwd, &path)?;

    if !full.is_file() {
        return Err(anyhow!("Path is not a file: {}", path));
    }

    let content = std::fs::read_to_string(&full)?;
    let lang = detect_language(&path);

    let mut suggestions = Vec::new();
    suggestions.push(format!("=== Refactoring Suggestions: {} ===", path));
    suggestions.push(String::new());

    // Check for code duplication (very basic)
    let lines: Vec<&str> = content.lines().collect();
    let mut duplicates = 0;
    for i in 0..lines.len().saturating_sub(5) {
        let chunk: String = lines[i..i + 5].join("\n");
        let chunk_count = content.matches(&chunk).count();
        if chunk_count > 1 {
            duplicates += 1;
        }
    }

    if duplicates > 3 {
        suggestions.push("🔄 Code Duplication:".to_string());
        suggestions.push("  - Multiple similar code blocks detected".to_string());
        suggestions.push("  - Consider extracting common logic into a function".to_string());
        suggestions.push(String::new());
    }

    // Check for long parameter lists
    if lang == "rust" || lang == "javascript" || lang == "typescript" {
        for (i, line) in lines.iter().enumerate() {
            if line.contains("fn ") || line.contains("function ") {
                let params = line.matches(',').count();
                if params > 4 {
                    suggestions.push(format!("📋 Long Parameter List (line {}): {} parameters", i + 1, params + 1));
                    suggestions.push("  - Consider using a struct/object for parameters".to_string());
                    suggestions.push(String::new());
                    break; // Only show first occurrence
                }
            }
        }
    }

    // Check for nested conditionals
    let max_indent = lines.iter()
        .map(|l| l.chars().take_while(|c| c.is_whitespace()).count())
        .max()
        .unwrap_or(0);

    if max_indent > 20 {
        suggestions.push("🌳 Deep Nesting:".to_string());
        suggestions.push(format!("  - Maximum indentation level: {}", max_indent));
        suggestions.push("  - Consider early returns or extracting nested logic".to_string());
        suggestions.push(String::new());
    }

    // General suggestions
    suggestions.push("💡 General Recommendations:".to_string());
    suggestions.push("  - Run a linter for detailed code quality analysis".to_string());
    suggestions.push("  - Consider adding unit tests if not present".to_string());
    suggestions.push("  - Review function and variable names for clarity".to_string());

    Ok(suggestions.join("\n"))
}
