use crate::cli::{
    CatalogArgs, Cli, DockerCommands, LoginArgs, RegistryAddArgs, RegistryCommands,
    RegistryRemoveArgs, RegistryUseArgs, TagsArgs,
};
use crate::config::{ConfigScope, ConfigStore, RegistrySource};
use crate::domain::{RegistryConfig, validate_registry_name};
use crate::registry_api::RegistryApi;
use anyhow::{Result, bail};

pub async fn run(cli: Cli) -> Result<()> {
    let store = ConfigStore::new();

    match cli.command.into_docker() {
        DockerCommands::Registry { command } => match command {
            RegistryCommands::Add(args) => cmd_registry_add(&store, args)?,
            RegistryCommands::List => cmd_registry_list(&store)?,
            RegistryCommands::Use(args) => cmd_registry_use(&store, args)?,
            RegistryCommands::Remove(args) => cmd_registry_remove(&store, args)?,
        },
        DockerCommands::Login(args) => cmd_docker_login(&store, args)?,
        DockerCommands::Catalog(args) => cmd_docker_catalog(&store, args).await?,
        DockerCommands::Tags(args) => cmd_docker_tags(&store, args).await?,
    }

    Ok(())
}

fn cmd_registry_add(store: &ConfigStore, args: RegistryAddArgs) -> Result<()> {
    validate_registry_name(&args.name)?;
    let scope = if args.global {
        ConfigScope::Global
    } else {
        ConfigScope::Local
    };
    store.ensure_layout(scope)?;

    let mut effective_root = store.load_effective_root_config()?;
    let mut reg = store
        .load_effective_registry_optional(&args.name)?
        .map(|entry| entry.config)
        .unwrap_or_default();

    reg.docker.url = args.url.trim().trim_end_matches('/').to_string();
    reg.docker.path = crate::domain::normalize_path(&args.path);
    reg.docker.service = args.service;
    reg.docker.insecure_tls = args.insecure_tls;

    store.save_registry(scope, &args.name, &reg)?;

    if args.r#use || effective_root.docker.current_registry.is_none() {
        effective_root.docker.current_registry = Some(args.name.clone());
        store.save_root_config(scope, &effective_root)?;
    }

    println!("registry '{}' saved", args.name);
    Ok(())
}

fn cmd_registry_list(store: &ConfigStore) -> Result<()> {
    let cfg = store.load_effective_root_config()?;
    let current = cfg.docker.current_registry.as_deref();

    let entries = store.list_effective_registries()?;
    if entries.is_empty() {
        println!("no registries configured");
        return Ok(());
    }

    for entry in entries {
        let marker = if Some(entry.name.as_str()) == current {
            "*"
        } else {
            " "
        };
        let source = match entry.source {
            RegistrySource::Local => "local",
            RegistrySource::Global => "global",
        };

        println!(
            "{} {} -> {}{} ({})",
            marker,
            entry.name,
            entry.config.docker.url,
            crate::domain::normalize_path(&entry.config.docker.path),
            source
        );
    }

    Ok(())
}

fn cmd_registry_use(store: &ConfigStore, args: RegistryUseArgs) -> Result<()> {
    validate_registry_name(&args.name)?;
    let scope = if args.global {
        ConfigScope::Global
    } else {
        ConfigScope::Local
    };
    if !store.registry_exists_in_scope(scope, &args.name) {
        bail!("registry '{}' does not exist", args.name);
    }

    let mut cfg = store.load_effective_root_config()?;
    cfg.docker.current_registry = Some(args.name.clone());
    store.save_root_config(scope, &cfg)?;

    println!("active registry set to '{}'", args.name);
    Ok(())
}

fn cmd_registry_remove(store: &ConfigStore, args: RegistryRemoveArgs) -> Result<()> {
    validate_registry_name(&args.name)?;
    let scope = if args.global {
        ConfigScope::Global
    } else {
        ConfigScope::Local
    };
    store.remove_registry(scope, &args.name)?;

    let mut cfg = store.load_effective_root_config()?;
    if cfg.docker.current_registry.as_deref() == Some(args.name.as_str()) {
        cfg.docker.current_registry = None;
        store.save_root_config(scope, &cfg)?;
    }

    println!("registry '{}' removed", args.name);
    Ok(())
}

fn cmd_docker_login(store: &ConfigStore, args: LoginArgs) -> Result<()> {
    let registry_name = store.resolve_registry_name(args.registry)?;
    let mut reg = store.load_effective_registry(&registry_name)?.config;

    reg.docker.username = Some(args.username);
    reg.docker.password = Some(args.password);

    let scope = if args.global {
        ConfigScope::Global
    } else {
        ConfigScope::Local
    };
    store.save_registry(scope, &registry_name, &reg)?;
    println!("credentials saved for '{}'", registry_name);
    Ok(())
}

async fn cmd_docker_catalog(store: &ConfigStore, args: CatalogArgs) -> Result<()> {
    let registry_name = store.resolve_registry_name(args.registry)?;
    let reg = store.load_effective_registry(&registry_name)?.config;

    let api = RegistryApi::new(&reg)?;
    let repositories = api.catalog(&reg, args.n).await?;

    println!("registry: {}", registry_name);
    for repository in repositories {
        println!("{}", repository);
    }

    Ok(())
}

async fn cmd_docker_tags(store: &ConfigStore, args: TagsArgs) -> Result<()> {
    let registry_name = store.resolve_registry_name(args.registry)?;
    let reg: RegistryConfig = store.load_effective_registry(&registry_name)?.config;

    let api = RegistryApi::new(&reg)?;
    let (name, tags) = api.tags(&reg, &args.repository, args.n).await?;

    println!("registry: {}", registry_name);
    println!("repository: {}", name);
    for tag in tags {
        println!("{}", tag);
    }

    Ok(())
}
