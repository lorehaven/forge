use anyhow::{Context, Result};
use clap::Parser;
use dotenvy::from_path;
use minijinja::{Environment, Value, context};
use regex::Regex;
use serde_yaml::Value as YamlValue;
use std::{collections::HashMap, fs, path::Path};

const OVERLAY_DIR: &str = "overlays";
const OUTPUT_DIR: &str = "manifests";

#[derive(Parser)]
struct Args {
    env: String,
    #[arg(long, default_value = OUTPUT_DIR)]
    output: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    generate_manifests(&args.env, &args.output)
}

fn load_env(env: &str) {
    let env_path = format!("{}/{}/.env", OVERLAY_DIR, env);
    if Path::new(&env_path).exists() {
        from_path(&env_path).ok();
    } else if Path::new(".env").exists() {
        from_path(".env").ok();
    }
}

fn load_embedded_templates(env: &mut Environment) {
    env.add_template("cronjob.yaml.j2", include_str!("templates/cronjob.yaml.j2"))
        .unwrap();
    env.add_template(
        "deployment.yaml.j2",
        include_str!("templates/deployment.yaml.j2"),
    )
    .unwrap();
    env.add_template(
        "ingressroute.yaml.j2",
        include_str!("templates/ingressroute.yaml.j2"),
    )
    .unwrap();
    env.add_template("job.yaml.j2", include_str!("templates/job.yaml.j2"))
        .unwrap();
    env.add_template(
        "middleware.yaml.j2",
        include_str!("templates/middleware.yaml.j2"),
    )
    .unwrap();
    env.add_template(
        "namespace.yaml.j2",
        include_str!("templates/namespace.yaml.j2"),
    )
    .unwrap();
    env.add_template("pv.yaml.j2", include_str!("templates/pv.yaml.j2"))
        .unwrap();
    env.add_template("pvc.yaml.j2", include_str!("templates/pvc.yaml.j2"))
        .unwrap();
    env.add_template("service.yaml.j2", include_str!("templates/service.yaml.j2"))
        .unwrap();
    env.add_template(
        "serviceaccount.yaml.j2",
        include_str!("templates/serviceaccount.yaml.j2"),
    )
    .unwrap();
}

fn substitute(value: &mut YamlValue, env: &HashMap<String, String>, re: &Regex) {
    match value {
        YamlValue::String(s) => {
            let new = re.replace_all(s, |caps: &regex::Captures| {
                env.get(&caps[1]).cloned().unwrap_or(caps[0].to_string())
            });
            *s = new.into_owned();
        }
        YamlValue::Mapping(map) => {
            for (_, v) in map.iter_mut() {
                substitute(v, env, re);
            }
        }
        YamlValue::Sequence(seq) => {
            for v in seq.iter_mut() {
                substitute(v, env, re);
            }
        }
        _ => {}
    }
}

fn generate_manifests(env_name: &str, output_dir: &str) -> Result<()> {
    load_env(env_name);

    let overlay_path = format!("{}/{}/overlay.yaml", OVERLAY_DIR, env_name);
    let overlay_src = fs::read_to_string(&overlay_path)
        .with_context(|| format!("Missing overlay {}", overlay_path))?;

    // --- Jinja: overlays ---
    let mut overlay_jinja = Environment::new();
    overlay_jinja.set_loader(minijinja::path_loader(OVERLAY_DIR));
    overlay_jinja.add_global("env", env_name);

    let rendered_overlay = overlay_jinja.render_str(&overlay_src, Value::UNDEFINED)?;
    let mut data: YamlValue = serde_yaml::from_str(&rendered_overlay)?;

    // --- substitute ${VAR} ---
    let env_map: HashMap<String, String> = std::env::vars().collect();
    let re = Regex::new(r"\$\{([^}]+)}")?;
    substitute(&mut data, &env_map, &re);

    // --- templates ---
    let mut tpl_env = Environment::new();
    load_embedded_templates(&mut tpl_env);
    tpl_env.add_global("env", env_name);

    let resources = data["resources"]
        .as_sequence()
        .context("resources must be a list")?;

    let mut rendered = Vec::new();

    for res in resources {
        let kind = res["kind"].as_str().context("resource.kind missing")?;
        let tpl = format!("{}.yaml.j2", kind.to_lowercase());

        let yaml = tpl_env.get_template(&tpl)?.render(context! {
            data => &data,
            res => res,
            env => env_name,
        })?;
        rendered.push(yaml.trim().to_string());
    }

    fs::create_dir_all(output_dir)?;
    let out = format!("{}/{}-manifests.yaml", output_dir, env_name);
    let full_yaml = rendered.join("\n---\n") + "\n";
    let cleaned = strip_empty_lines(&full_yaml);
    fs::write(out.clone(), cleaned)?;

    println!("Generated {out}\nApply with: kubectl apply -f {out}");

    Ok(())
}

fn strip_empty_lines(s: &str) -> String {
    s.lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}
