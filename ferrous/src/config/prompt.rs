use anyhow::{Context, Result};
use minijinja::{Environment, context};
use serde::Serialize;
use std::fmt;

pub struct PromptManager {
    env: Environment<'static>,
}

impl fmt::Debug for PromptManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PromptManager").finish()
    }
}

#[derive(Serialize, Debug)]
pub struct PromptContext {
    pub date: String,
    pub os: String,
    pub project_name: String,
}

impl PromptManager {
    pub fn new() -> Result<Self> {
        let mut env = Environment::new();

        let rules_dir = std::env::current_dir()
            .context("Failed to get current directory")?
            .join(".ferrous")
            .join("rules");

        if rules_dir.exists() {
            env.set_loader(minijinja::path_loader(rules_dir));
        }

        Ok(Self { env })
    }

    pub fn render_system(&self, context: &PromptContext) -> Result<String> {
        self.render("system.md", context)
            .or_else(|_| Ok(crate::core::agent::DEFAULT_PROMPT.to_string()))
    }

    pub fn render_planner(&self, context: &PromptContext) -> Result<String> {
        self.render("planner.md", context)
            .or_else(|_| Ok(crate::core::agent::DEFAULT_PLAN_PROMPT.to_string()))
    }

    fn render(&self, template_name: &str, ctx: &PromptContext) -> Result<String> {
        let tmpl = self
            .env
            .get_template(template_name)
            .context(format!("Template {template_name} not found"))?;

        tmpl.render(context!(
            date => ctx.date,
            os => ctx.os,
            project_name => ctx.project_name,
        ))
        .context(format!("Failed to render template {template_name}"))
    }
}

#[must_use]
pub fn get_default_context() -> PromptContext {
    PromptContext {
        date: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        os: std::env::consts::OS.to_string(),
        project_name: std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "unknown".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_system_fallback() {
        let pm = PromptManager {
            env: Environment::new(),
        };
        let ctx = PromptContext {
            date: "2024-01-01".to_string(),
            os: "linux".to_string(),
            project_name: "test".to_string(),
        };
        let rendered = pm.render_system(&ctx).unwrap();
        assert!(rendered.contains("You are Ferrous"));
    }

    #[test]
    fn test_render_system_custom() {
        let mut env = Environment::new();
        env.add_template("system.md", "Hello {{ project_name }} on {{ os }}")
            .unwrap();
        let pm = PromptManager { env };
        let ctx = PromptContext {
            date: "2024-01-01".to_string(),
            os: "linux".to_string(),
            project_name: "ferrous".to_string(),
        };
        let rendered = pm.render_system(&ctx).unwrap();
        assert_eq!(rendered, "Hello ferrous on linux");
    }
}
