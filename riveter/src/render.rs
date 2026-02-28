use crate::env::{OUTPUT_DIR, OVERLAY_DIR, manifest_path};
use anyhow::Context;
use dotenvy::from_path;
use minijinja::{Environment, Value, context};
use regex::Regex;
use serde_yaml::Value as YamlValue;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceScope {
    Mutable,
    Immutable,
    All,
}

#[derive(Debug, Clone)]
pub struct RenderedManifest {
    pub path: String,
    pub resource_count: usize,
}

pub fn generate_manifests(env_name: &str) -> anyhow::Result<String> {
    Ok(generate_manifests_with_scope(env_name, ResourceScope::All)?.path)
}

pub fn generate_manifests_with_scope(
    env_name: &str,
    scope: ResourceScope,
) -> anyhow::Result<RenderedManifest> {
    load_env(env_name);

    let data = render_overlay(env_name)?;
    let rendered_resources = render_resources(env_name, &data, scope)?;

    fs::create_dir_all(OUTPUT_DIR)?;
    let path = scoped_manifest_path(env_name, scope);
    fs::write(
        &path,
        if rendered_resources.is_empty() {
            String::new()
        } else {
            strip_empty_lines(&(rendered_resources.join("\n---\n") + "\n"))
        },
    )?;

    Ok(RenderedManifest {
        path,
        resource_count: rendered_resources.len(),
    })
}

fn render_overlay(env_name: &str) -> anyhow::Result<YamlValue> {
    let overlay_src = fs::read_to_string(format!("{OVERLAY_DIR}/{env_name}/overlay.yaml"))?;

    let mut overlay_jinja = Environment::new();
    overlay_jinja.set_loader(minijinja::path_loader(OVERLAY_DIR));
    overlay_jinja.add_global("env", env_name);

    let rendered_overlay = overlay_jinja.render_str(&overlay_src, Value::UNDEFINED)?;
    let mut data: YamlValue = serde_yaml::from_str(&rendered_overlay)?;

    let re = Regex::new(r"\$\{([^}]+)}")?;
    substitute(&mut data, &std::env::vars().collect(), &re);

    Ok(data)
}

fn render_resources(
    env_name: &str,
    data: &YamlValue,
    scope: ResourceScope,
) -> anyhow::Result<Vec<String>> {
    let resources = data["resources"]
        .as_sequence()
        .context("resources must be a list")?;

    let mut tpl_env = Environment::new();
    load_embedded_templates(&mut tpl_env);
    tpl_env.add_global("env", env_name);

    let mut out = Vec::new();
    for res in resources {
        if !resource_in_scope(res, scope) {
            continue;
        }

        let kind = res["kind"].as_str().context("kind missing")?;
        let tpl = format!("{}.yaml.j2", kind.to_lowercase());
        let y = tpl_env.get_template(&tpl)?.render(context! {
            data => data,
            res => res,
            env => env_name,
        })?;
        out.push(y.trim().to_string());
    }
    Ok(out)
}

fn scoped_manifest_path(env: &str, scope: ResourceScope) -> String {
    match scope {
        ResourceScope::All => manifest_path(env),
        ResourceScope::Mutable => format!("{OUTPUT_DIR}/{env}-manifests.mutable.yaml"),
        ResourceScope::Immutable => format!("{OUTPUT_DIR}/{env}-manifests.immutable.yaml"),
    }
}

fn resource_in_scope(res: &YamlValue, scope: ResourceScope) -> bool {
    match scope {
        ResourceScope::All => true,
        ResourceScope::Mutable => !resource_is_immutable(res),
        ResourceScope::Immutable => resource_is_immutable(res),
    }
}

fn resource_is_immutable(res: &YamlValue) -> bool {
    if res["immutable"].as_bool().unwrap_or(false) {
        return true;
    }

    if let Some(lifecycle) = res["lifecycle"].as_str() {
        let lifecycle = lifecycle.trim();
        return lifecycle.eq_ignore_ascii_case("immutable")
            || lifecycle.eq_ignore_ascii_case("static");
    }

    false
}

fn load_env(env: &str) {
    let env_path = format!("{OVERLAY_DIR}/{env}/.env");
    if Path::new(&env_path).exists() {
        from_path(&env_path).ok();
    } else if Path::new(".env").exists() {
        from_path(".env").ok();
    }
}

fn load_embedded_templates(env: &mut Environment<'_>) {
    let templates = [
        "cronjob.yaml.j2",
        "deployment.yaml.j2",
        "ingress.yaml.j2",
        "ingressroute.yaml.j2",
        "job.yaml.j2",
        "middleware.yaml.j2",
        "namespace.yaml.j2",
        "pv.yaml.j2",
        "pvc.yaml.j2",
        "service.yaml.j2",
        "serviceaccount.yaml.j2",
    ];

    for tpl in templates {
        env.add_template(tpl, get_template_source(tpl)).unwrap();
    }
}

fn get_template_source(name: &str) -> &'static str {
    match name {
        "cronjob.yaml.j2" => include_str!("templates/cronjob.yaml.j2"),
        "deployment.yaml.j2" => include_str!("templates/deployment.yaml.j2"),
        "ingress.yaml.j2" => include_str!("templates/ingress.yaml.j2"),
        "ingressroute.yaml.j2" => include_str!("templates/ingressroute.yaml.j2"),
        "job.yaml.j2" => include_str!("templates/job.yaml.j2"),
        "middleware.yaml.j2" => include_str!("templates/middleware.yaml.j2"),
        "namespace.yaml.j2" => include_str!("templates/namespace.yaml.j2"),
        "pv.yaml.j2" => include_str!("templates/pv.yaml.j2"),
        "pvc.yaml.j2" => include_str!("templates/pvc.yaml.j2"),
        "service.yaml.j2" => include_str!("templates/service.yaml.j2"),
        "serviceaccount.yaml.j2" => include_str!("templates/serviceaccount.yaml.j2"),
        _ => panic!("Unknown template: {name}"),
    }
}

fn substitute(value: &mut YamlValue, env: &HashMap<String, String>, re: &Regex) {
    match value {
        YamlValue::String(s) => {
            *s = re
                .replace_all(s, |c: &regex::Captures<'_>| {
                    env.get(&c[1]).cloned().unwrap_or_else(|| c[0].to_string())
                })
                .into_owned();
        }
        YamlValue::Mapping(m) => m.values_mut().for_each(|v| substitute(v, env, re)),
        YamlValue::Sequence(s) => s.iter_mut().for_each(|v| substitute(v, env, re)),
        _ => {}
    }
}

#[must_use]
pub fn strip_empty_lines(s: &str) -> String {
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[cfg(test)]
mod tests {
    use super::{ResourceScope, resource_in_scope};
    use serde_yaml::Value as YamlValue;

    fn parse_resource(yaml: &str) -> YamlValue {
        serde_yaml::from_str(yaml).expect("resource yaml should parse")
    }

    #[test]
    fn immutable_flag_marks_resource_immutable() {
        let res = parse_resource("kind: namespace\nimmutable: true\n");
        assert!(!resource_in_scope(&res, ResourceScope::Mutable));
        assert!(resource_in_scope(&res, ResourceScope::Immutable));
    }

    #[test]
    fn lifecycle_static_marks_resource_immutable() {
        let res = parse_resource("kind: ingress\nlifecycle: static\n");
        assert!(!resource_in_scope(&res, ResourceScope::Mutable));
        assert!(resource_in_scope(&res, ResourceScope::Immutable));
    }

    #[test]
    fn default_resources_are_mutable() {
        let res = parse_resource("kind: deployment\n");
        assert!(resource_in_scope(&res, ResourceScope::Mutable));
        assert!(!resource_in_scope(&res, ResourceScope::Immutable));
    }
}
