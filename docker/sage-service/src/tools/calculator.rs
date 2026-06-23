use super::{ToolCall, ToolDefinition, ToolExecutor, ToolParameters, ToolResult};
use async_trait::async_trait;
use serde_json::json;

pub fn get_definition() -> ToolDefinition {
    ToolDefinition {
        name: "calculator".to_string(),
        description: "Perform mathematical calculations".to_string(),
        tool_type: "function".to_string(),
        parameters: ToolParameters {
            param_type: "object".to_string(),
            properties: json!({
                "expression": {
                    "type": "string",
                    "description": "Mathematical expression to evaluate (e.g., '2 + 2', '10 * 5', 'sqrt(16)')"
                }
            }),
            required: vec!["expression".to_string()],
        },
    }
}

pub struct CalculatorExecutor;

#[async_trait]
impl ToolExecutor for CalculatorExecutor {
    async fn execute(&self, tool_call: &ToolCall) -> ToolResult {
        let expression = match tool_call.arguments.get("expression") {
            Some(val) => match val.as_str() {
                Some(s) => s.to_string(),
                None => {
                    return ToolResult {
                        tool_use_id: tool_call.id.clone(),
                        content: "Invalid expression: must be a string".to_string(),
                        is_error: true,
                    }
                }
            },
            None => {
                return ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content: "Missing 'expression' argument".to_string(),
                    is_error: true,
                }
            }
        };

        match eval_expression(&expression) {
            Ok(result) => {
                // Format the result with proper formatting
                let formatted_result = if result.fract() == 0.0 {
                    format!("{:.0}", result)
                } else {
                    format!("{}", result)
                };

                let content = format!(
                    "**Calculation Result**\n\n\
                     Expression: `{}`\n\n\
                     Result: **{}**\n\n\
                     (Use this result in further calculations or show to the user)",
                    expression,
                    formatted_result
                );

                ToolResult {
                    tool_use_id: tool_call.id.clone(),
                    content,
                    is_error: false,
                }
            }
            Err(err) => ToolResult {
                tool_use_id: tool_call.id.clone(),
                content: format!("Calculation error: {}", err),
                is_error: true,
            },
        }
    }
}

fn eval_expression(expr: &str) -> Result<f64, String> {
    let expr = expr.trim().to_lowercase();

    // Simple mathematical expression evaluator
    // Supports: +, -, *, /, %, ^, sqrt(), abs(), sin(), cos(), tan(), ln(), log()
    eval_expr(&expr)
}

fn eval_expr(expr: &str) -> Result<f64, String> {
    let expr = expr.replace(" ", "");
    eval_additive(&expr, &mut 0)
}

fn eval_additive(expr: &str, pos: &mut usize) -> Result<f64, String> {
    let mut result = eval_multiplicative(expr, pos)?;

    while *pos < expr.len() {
        match expr.chars().nth(*pos) {
            Some('+') => {
                *pos += 1;
                result += eval_multiplicative(expr, pos)?;
            }
            Some('-') => {
                *pos += 1;
                result -= eval_multiplicative(expr, pos)?;
            }
            _ => break,
        }
    }

    Ok(result)
}

fn eval_multiplicative(expr: &str, pos: &mut usize) -> Result<f64, String> {
    let mut result = eval_power(expr, pos)?;

    while *pos < expr.len() {
        match expr.chars().nth(*pos) {
            Some('*') => {
                *pos += 1;
                result *= eval_power(expr, pos)?;
            }
            Some('/') => {
                *pos += 1;
                let divisor = eval_power(expr, pos)?;
                if divisor == 0.0 {
                    return Err("Division by zero".to_string());
                }
                result /= divisor;
            }
            Some('%') => {
                *pos += 1;
                result %= eval_power(expr, pos)?;
            }
            _ => break,
        }
    }

    Ok(result)
}

fn eval_power(expr: &str, pos: &mut usize) -> Result<f64, String> {
    let mut result = eval_unary(expr, pos)?;

    while *pos < expr.len() && expr.chars().nth(*pos) == Some('^') {
        *pos += 1;
        let exponent = eval_unary(expr, pos)?;
        result = result.powf(exponent);
    }

    Ok(result)
}

fn eval_unary(expr: &str, pos: &mut usize) -> Result<f64, String> {
    if *pos >= expr.len() {
        return Err("Unexpected end of expression".to_string());
    }

    match expr.chars().nth(*pos) {
        Some('-') => {
            *pos += 1;
            Ok(-eval_unary(expr, pos)?)
        }
        Some('+') => {
            *pos += 1;
            eval_unary(expr, pos)
        }
        _ => eval_primary(expr, pos),
    }
}

fn eval_primary(expr: &str, pos: &mut usize) -> Result<f64, String> {
    // Check for functions
    let remaining = &expr[*pos..];
    if remaining.starts_with("sqrt(") {
        *pos += 5;
        let val = eval_expr_until(expr, pos, ')')?;
        *pos += 1;
        return Ok(val.sqrt());
    }
    if remaining.starts_with("abs(") {
        *pos += 4;
        let val = eval_expr_until(expr, pos, ')')?;
        *pos += 1;
        return Ok(val.abs());
    }
    if remaining.starts_with("sin(") {
        *pos += 4;
        let val = eval_expr_until(expr, pos, ')')?;
        *pos += 1;
        return Ok(val.sin());
    }
    if remaining.starts_with("cos(") {
        *pos += 4;
        let val = eval_expr_until(expr, pos, ')')?;
        *pos += 1;
        return Ok(val.cos());
    }
    if remaining.starts_with("tan(") {
        *pos += 4;
        let val = eval_expr_until(expr, pos, ')')?;
        *pos += 1;
        return Ok(val.tan());
    }
    if remaining.starts_with("ln(") {
        *pos += 3;
        let val = eval_expr_until(expr, pos, ')')?;
        *pos += 1;
        return Ok(val.ln());
    }
    if remaining.starts_with("log(") {
        *pos += 4;
        let val = eval_expr_until(expr, pos, ')')?;
        *pos += 1;
        return Ok(val.log10());
    }

    // Check for parentheses
    if remaining.starts_with('(') {
        *pos += 1;
        let val = eval_expr_until(expr, pos, ')')?;
        *pos += 1;
        return Ok(val);
    }

    // Parse number
    eval_number(expr, pos)
}

fn eval_expr_until(expr: &str, pos: &mut usize, terminator: char) -> Result<f64, String> {
    let start = *pos;
    let mut depth = 0;

    while *pos < expr.len() {
        let c = expr.chars().nth(*pos).unwrap();
        if c == '(' || c == '[' {
            depth += 1;
        } else if c == ')' || c == ']' {
            if depth == 0 && c == terminator {
                break;
            }
            depth -= 1;
        }
        *pos += 1;
    }

    if *pos == start {
        return Err("Empty expression".to_string());
    }

    let sub_expr = &expr[start..*pos];
    eval_expr(sub_expr)
}

fn eval_number(expr: &str, pos: &mut usize) -> Result<f64, String> {
    let start = *pos;
    let mut has_dot = false;

    while *pos < expr.len() {
        match expr.chars().nth(*pos) {
            Some(c) if c.is_numeric() => {
                *pos += 1;
            }
            Some('.') if !has_dot => {
                has_dot = true;
                *pos += 1;
            }
            _ => break,
        }
    }

    if *pos == start {
        return Err("Expected number".to_string());
    }

    expr[start..*pos].parse().map_err(|_| "Invalid number".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        assert_eq!(eval_expression("2 + 2").unwrap(), 4.0);
        assert_eq!(eval_expression("10 - 3").unwrap(), 7.0);
        assert_eq!(eval_expression("4 * 5").unwrap(), 20.0);
        assert_eq!(eval_expression("20 / 4").unwrap(), 5.0);
    }

    #[test]
    fn test_complex_expressions() {
        assert_eq!(eval_expression("2 + 3 * 4").unwrap(), 14.0);
        assert_eq!(eval_expression("(2 + 3) * 4").unwrap(), 20.0);
        assert_eq!(eval_expression("2 ^ 3").unwrap(), 8.0);
    }

    #[test]
    fn test_functions() {
        assert!((eval_expression("sqrt(16)").unwrap() - 4.0).abs() < 0.001);
        assert!((eval_expression("abs(-5)").unwrap() - 5.0).abs() < 0.001);
    }
}
