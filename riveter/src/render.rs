use crate::env::{OUTPUT_DIR, OVERLAY_DIR, manifest_path};
use anyhow::Context;
use dotenvy::from_path;
use minijinja::{Environment, Value, context};
use regex::Regex;
use serde_yaml::Value as YamlValue;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub fn generate_manifests(env_name: &str) -> anyhow::Result<String> {
    load_env(env_name);

    let data = render_overlay(env_name)?;
    let rendered_resources = render_resources(env_name, &data)?;

    fs::create_dir_all(OUTPUT_DIR)?;
    let path = manifest_path(env_name);
    fs::write(
        &path,
        strip_empty_lines(&(rendered_resources.join("\n---\n") + "\n")),
    )?;

    Ok(path)
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

fn render_resources(env_name: &str, data: &YamlValue) -> anyhow::Result<Vec<String>> {
    let resources = data["resources"]
        .as_sequence()
        .context("resources must be a list")?;

    let mut tpl_env = Environment::new();
    load_embedded_templates(&mut tpl_env);
    tpl_env.add_global("env", env_name);

    let mut out = Vec::new();
    for res in resources {
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

fn strip_empty_lines(s: &str) -> String {
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}
