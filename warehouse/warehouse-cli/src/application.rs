use crate::cli::{
    CatalogArgs, Cli, Commands, CratesCommands, CratesLoginArgs, CratesRegistryAddArgs,
    CratesRegistryCommands, CratesRegistryRemoveArgs, CratesRegistryUseArgs, CratesSearchArgs,
    CratesUnyankArgs, CratesVersionsArgs, CratesYankArgs, DockerCommands, LoginArgs,
    RegistryAddArgs, RegistryCommands, RegistryRemoveArgs, RegistryUseArgs, TagsArgs,
};
use crate::config::{ConfigScope, ConfigStore, RegistrySource};
use crate::crates_api::CratesApi;
use crate::docker_api::DockerApi;
use crate::domain::{RegistryConfig, validate_registry_name};
use anyhow::{Result, bail};

pub async fn run(cli: Cli) -> Result<()> {
    let store = ConfigStore::new();

    match cli.command {
        Commands::Docker { command } => run_docker(&store, command).await,
        Commands::Crates { command } => run_crates(&store, command).await,
    }
}

// ---------------------------------------------------------------------------
// Docker dispatch
// ---------------------------------------------------------------------------

async fn run_docker(store: &ConfigStore, command: DockerCommands) -> Result<()> {
    match command {
        DockerCommands::Registry { command } => match command {
            RegistryCommands::Add(args) => cmd_registry_add(store, args)?,
            RegistryCommands::List => cmd_registry_list(store)?,
            RegistryCommands::Use(args) => cmd_registry_use(store, args)?,
            RegistryCommands::Remove(args) => cmd_registry_remove(store, args)?,
        },
        DockerCommands::Login(args) => cmd_docker_login(store, args)?,
        DockerCommands::Catalog(args) => cmd_docker_catalog(store, args).await?,
        DockerCommands::Tags(args) => cmd_docker_tags(store, args).await?,
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Crates dispatch
// ---------------------------------------------------------------------------

async fn run_crates(store: &ConfigStore, command: CratesCommands) -> Result<()> {
    match command {
        CratesCommands::Registry { command } => match command {
            CratesRegistryCommands::Add(args) => cmd_crates_registry_add(store, args)?,
            CratesRegistryCommands::List => cmd_crates_registry_list(store)?,
            CratesRegistryCommands::Use(args) => cmd_crates_registry_use(store, args)?,
            CratesRegistryCommands::Remove(args) => cmd_crates_registry_remove(store, args)?,
        },
        CratesCommands::Login(args) => cmd_crates_login(store, args)?,
        CratesCommands::Search(args) => cmd_crates_search(store, args).await?,
        CratesCommands::Versions(args) => cmd_crates_versions(store, args).await?,
        CratesCommands::Yank(args) => cmd_crates_yank(store, args).await?,
        CratesCommands::Unyank(args) => cmd_crates_unyank(store, args).await?,
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Docker commands  (unchanged from original)
// ---------------------------------------------------------------------------

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

    let api = DockerApi::new(&reg)?;
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

    let api = DockerApi::new(&reg)?;
    let (name, tags) = api.tags(&reg, &args.repository, args.n).await?;

    println!("registry: {}", registry_name);
    println!("repository: {}", name);
    for tag in tags {
        println!("{}", tag);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Crates commands
// ---------------------------------------------------------------------------

fn cmd_crates_registry_add(store: &ConfigStore, args: CratesRegistryAddArgs) -> Result<()> {
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
        .map(|e| e.config)
        .unwrap_or_default();

    reg.crates.url = args.url.trim().trim_end_matches('/').to_string();
    reg.crates.insecure_tls = args.insecure_tls;

    store.save_registry(scope, &args.name, &reg)?;

    if args.r#use || effective_root.crates.current_registry.is_none() {
        effective_root.crates.current_registry = Some(args.name.clone());
        store.save_root_config(scope, &effective_root)?;
    }

    println!("crates registry '{}' saved", args.name);
    Ok(())
}

fn cmd_crates_registry_list(store: &ConfigStore) -> Result<()> {
    let cfg = store.load_effective_root_config()?;
    let current = cfg.crates.current_registry.as_deref();

    let entries = store.list_effective_registries()?;

    let crates_entries: Vec<_> = entries
        .into_iter()
        .filter(|e| !e.config.crates.url.is_empty())
        .collect();

    if crates_entries.is_empty() {
        println!("no crates registries configured");
        return Ok(());
    }

    for entry in crates_entries {
        let marker = if Some(entry.name.as_str()) == current {
            "*"
        } else {
            " "
        };
        let source = match entry.source {
            RegistrySource::Local => "local",
            RegistrySource::Global => "global",
        };
        let authed = if entry.config.crates.token.is_some() {
            " [token set]"
        } else {
            ""
        };

        println!(
            "{} {} -> {}{} ({})",
            marker, entry.name, entry.config.crates.url, authed, source
        );
    }

    Ok(())
}

fn cmd_crates_registry_use(store: &ConfigStore, args: CratesRegistryUseArgs) -> Result<()> {
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
    cfg.crates.current_registry = Some(args.name.clone());
    store.save_root_config(scope, &cfg)?;

    println!("active crates registry set to '{}'", args.name);
    Ok(())
}

fn cmd_crates_registry_remove(store: &ConfigStore, args: CratesRegistryRemoveArgs) -> Result<()> {
    validate_registry_name(&args.name)?;
    let scope = if args.global {
        ConfigScope::Global
    } else {
        ConfigScope::Local
    };
    store.remove_registry(scope, &args.name)?;

    let mut cfg = store.load_effective_root_config()?;
    if cfg.crates.current_registry.as_deref() == Some(args.name.as_str()) {
        cfg.crates.current_registry = None;
        store.save_root_config(scope, &cfg)?;
    }

    println!("crates registry '{}' removed", args.name);
    Ok(())
}

fn cmd_crates_login(store: &ConfigStore, args: CratesLoginArgs) -> Result<()> {
    let registry_name = store.resolve_crates_registry_name(args.registry)?;
    let mut reg = store.load_effective_registry(&registry_name)?.config;

    reg.crates.token = Some(args.token);

    let scope = if args.global {
        ConfigScope::Global
    } else {
        ConfigScope::Local
    };
    store.save_registry(scope, &registry_name, &reg)?;
    println!("token saved for crates registry '{}'", registry_name);
    Ok(())
}

async fn cmd_crates_search(store: &ConfigStore, args: CratesSearchArgs) -> Result<()> {
    let registry_name = store.resolve_crates_registry_name(args.registry)?;
    let reg = store.load_effective_registry(&registry_name)?.config;

    let api = CratesApi::new(&reg.crates)?;
    let (crates, total) = api.search(&reg.crates, &args.query, args.limit).await?;

    println!("registry: {}", registry_name);
    println!("query: \"{}\"  ({} total)", args.query, total);
    println!();

    if crates.is_empty() {
        println!("no crates found");
        return Ok(());
    }

    // Align name column
    let max_name = crates.iter().map(|c| c.name.len()).max().unwrap_or(0);
    let max_ver = crates
        .iter()
        .map(|c| c.max_version.len())
        .max()
        .unwrap_or(0);

    for c in &crates {
        let desc = c.description.as_deref().unwrap_or("");
        println!(
            "{:<name_w$}  {:<ver_w$}  {}",
            c.name,
            c.max_version,
            desc,
            name_w = max_name,
            ver_w = max_ver,
        );
    }

    Ok(())
}

async fn cmd_crates_versions(store: &ConfigStore, args: CratesVersionsArgs) -> Result<()> {
    let registry_name = store.resolve_crates_registry_name(args.registry)?;
    let reg = store.load_effective_registry(&registry_name)?.config;

    let api = CratesApi::new(&reg.crates)?;
    let records = api.versions(&reg.crates, &args.crate_name).await?;

    println!("registry: {}", registry_name);
    println!("crate: {}", args.crate_name);
    println!();

    let to_show: Vec<_> = if args.all {
        records.iter().collect()
    } else {
        records.iter().filter(|r| !r.yanked).collect()
    };

    if to_show.is_empty() {
        if args.all {
            println!("no versions found");
        } else {
            println!("no active versions (use --all to include yanked)");
        }
        return Ok(());
    }

    // Header
    println!("{:<20}  {:<8}  checksum", "version", "status");
    println!("{}", "-".repeat(72));

    for r in to_show.iter().rev() {
        let status = if r.yanked { "yanked" } else { "active" };
        let short_cksum = if r.cksum.len() > 16 {
            format!("{}…", &r.cksum[..16])
        } else {
            r.cksum.clone()
        };
        println!("{:<20}  {:<8}  {}", r.vers, status, short_cksum);
    }

    Ok(())
}

async fn cmd_crates_yank(store: &ConfigStore, args: CratesYankArgs) -> Result<()> {
    let registry_name = store.resolve_crates_registry_name(args.registry)?;
    let reg = store.load_effective_registry(&registry_name)?.config;

    if reg.crates.token.is_none() {
        bail!(
            "no token set for '{}'; run `warehouse crates login --token <token>`",
            registry_name
        );
    }

    let api = CratesApi::new(&reg.crates)?;
    api.yank(&reg.crates, &args.crate_name, &args.version)
        .await?;

    println!(
        "yanked {}-{} from '{}'",
        args.crate_name, args.version, registry_name
    );
    Ok(())
}

async fn cmd_crates_unyank(store: &ConfigStore, args: CratesUnyankArgs) -> Result<()> {
    let registry_name = store.resolve_crates_registry_name(args.registry)?;
    let reg = store.load_effective_registry(&registry_name)?.config;

    if reg.crates.token.is_none() {
        bail!(
            "no token set for '{}'; run `warehouse crates login --token <token>`",
            registry_name
        );
    }

    let api = CratesApi::new(&reg.crates)?;
    api.unyank(&reg.crates, &args.crate_name, &args.version)
        .await?;

    println!(
        "unyanked {}-{} in '{}'",
        args.crate_name, args.version, registry_name
    );
    Ok(())
}
