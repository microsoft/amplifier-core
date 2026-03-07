#[allow(warnings)]
mod bindings;

use amplifier_guest::{Tool, ToolResult, ToolSpec, Value};
use std::collections::HashMap;

#[derive(Default)]
struct CalculatorTool;

impl Tool for CalculatorTool {
    fn name(&self) -> &str {
        "calculator"
    }

    fn get_spec(&self) -> ToolSpec {
        let mut params = HashMap::new();
        params.insert("type".to_string(), serde_json::json!("object"));
        params.insert(
            "properties".to_string(),
            serde_json::json!({"expression": {"type": "string", "description": "A simple math expression like '2 + 3' or '10 / 5'"}}),
        );
        params.insert(
            "required".to_string(),
            serde_json::json!(["expression"]),
        );
        ToolSpec {
            name: "calculator".to_string(),
            parameters: params,
            description: Some(
                "Evaluates simple math expressions (a op b) supporting +, -, *, /".to_string(),
            ),
        }
    }

    fn execute(&self, input: Value) -> Result<ToolResult, String> {
        let expression = input
            .get("expression")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing 'expression' string parameter".to_string())?;

        match eval_simple(expression) {
            Ok(result) => Ok(ToolResult {
                success: true,
                output: Some(serde_json::json!({ "result": result })),
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: None,
                error: Some({
                    let mut m = HashMap::new();
                    m.insert("message".to_string(), serde_json::json!(e));
                    m
                }),
            }),
        }
    }
}

/// Evaluate a simple "a op b" expression.
///
/// Supports +, -, *, / operators with f64 operands.
/// Returns an error for division by zero or malformed expressions.
fn eval_simple(expr: &str) -> Result<f64, String> {
    let parts: Vec<&str> = expr.trim().split_whitespace().collect();
    if parts.len() != 3 {
        return Err(format!(
            "expected 'a op b' format (3 tokens), got {} tokens",
            parts.len()
        ));
    }

    let a: f64 = parts[0]
        .parse()
        .map_err(|_| format!("invalid number: {}", parts[0]))?;
    let op = parts[1];
    let b: f64 = parts[2]
        .parse()
        .map_err(|_| format!("invalid number: {}", parts[2]))?;

    match op {
        "+" => Ok(a + b),
        "-" => Ok(a - b),
        "*" => Ok(a * b),
        "/" => {
            if b == 0.0 {
                Err("division by zero".to_string())
            } else {
                Ok(a / b)
            }
        }
        _ => Err(format!("unsupported operator: {op}")),
    }
}

amplifier_guest::export_tool!(CalculatorTool);
