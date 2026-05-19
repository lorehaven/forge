use crate::api::admin_api::AdminApi;
use crate::api::crates_api::CratesApi;
use crate::api::docker_api::DockerApi;
use crate::api::files_api::{FilesApi, remote_path_for_upload};
use crate::cli::{
    AdminCommands, AdminGcArgs, CatalogArgs, Cli, Commands, CratesCommands, CratesLoginArgs,
    CratesRegistryAddArgs, CratesRegistryCommands, CratesRegistryRemoveArgs, CratesRegistryUseArgs,
    CratesSearchArgs, CratesUnyankArgs, CratesVersionsArgs, CratesYankArgs, DockerCommands,
    FilesBulkDeleteArgs, FilesBulkDownloadArgs, FilesCommands, FilesDeleteArgs, FilesDownloadArgs,
    FilesLsArgs, FilesMkdirArgs, FilesPreviewArgs, FilesRmdirArgs, FilesStoragesArgs,
    FilesUploadArgs, LoginArgs, RegistryAddArgs, RegistryCommands, RegistryRemoveArgs,
    RegistryUseArgs, TagsArgs,
};
use crate::config::{ConfigScope, ConfigStore, RegistrySource};
use crate::domain::{RegistryConfig, normalize_base_path, validate_registry_name};
use crate::ui;
use anyhow::{Result, bail};

macro_rules! qprintln {
    () => {{
        ui::line("");
    }};
    ($($arg:tt)*) => {{
        ui::line(format!($($arg)*));
    }};
}

pub async fn run(cli: Cli) -> Result<()> {
    let store = ConfigStore::new();

    match cli.command {
        Commands::Docker { command } => run_docker(&store, command).await,
        Commands::Crates { command } => run_crates(&store, command).await,
        Commands::Files { command } => run_files(&store, command).await,
        Commands::Admin { command } => run_admin(&store, command).await,
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

async fn run_files(store: &ConfigStore, command: FilesCommands) -> Result<()> {
    match command {
        FilesCommands::Storages(args) => cmd_files_storages(store, args).await?,
        FilesCommands::Ls(args) => cmd_files_ls(store, args).await?,
        FilesCommands::Upload(args) => cmd_files_upload(store, args).await?,
        FilesCommands::Preview(args) => cmd_files_preview(store, args).await?,
        FilesCommands::Download(args) => cmd_files_download(store, args).await?,
        FilesCommands::Mkdir(args) => cmd_files_mkdir(store, args).await?,
        FilesCommands::Rmdir(args) => cmd_files_rmdir(store, args).await?,
        FilesCommands::Delete(args) => cmd_files_delete(store, args).await?,
        FilesCommands::BulkDelete(args) => cmd_files_bulk_delete(store, args).await?,
        FilesCommands::BulkDownload(args) => cmd_files_bulk_download(store, args).await?,
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Docker commands
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
    reg.base_path = normalize_base_path(&args.base_path);
    reg.docker.path = crate::domain::normalize_path(&args.path);
    reg.docker.service = args.service;
    reg.docker.insecure_tls = args.insecure_tls;

    store.save_registry(scope, &args.name, &reg)?;

    if args.r#use || effective_root.docker.current_registry.is_none() {
        effective_root.docker.current_registry = Some(args.name.clone());
        store.save_root_config(scope, &effective_root)?;
    }

    ui::ok(format!("registry '{}' saved", args.name));
    Ok(())
}

fn cmd_registry_list(store: &ConfigStore) -> Result<()> {
    let cfg = store.load_effective_root_config()?;
    let current = cfg.docker.current_registry.as_deref();

    let entries = store.list_effective_registries()?;
    if entries.is_empty() {
        ui::warn("no registries configured");
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

        let base_path = if entry.config.base_path.is_empty() {
            String::new()
        } else {
            format!(" base-path={}", entry.config.base_path)
        };

        qprintln!(
            "{} {} -> {}{}{} ({})",
            marker,
            entry.name,
            entry.config.docker.url,
            crate::domain::normalize_path(&entry.config.docker.path),
            base_path,
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

    ui::ok(format!("active registry set to '{}'", args.name));
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

    ui::ok(format!("registry '{}' removed", args.name));
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
    ui::ok(format!("credentials saved for '{}'", registry_name));
    Ok(())
}

async fn cmd_docker_catalog(store: &ConfigStore, args: CatalogArgs) -> Result<()> {
    let registry_name = store.resolve_registry_name(args.registry)?;
    let reg = store.load_effective_registry(&registry_name)?.config;

    let api = DockerApi::new(&reg)?;
    let repositories = api.catalog(&reg, args.n).await?;

    qprintln!("registry: {}", registry_name);
    for repository in repositories {
        qprintln!("{}", repository);
    }

    Ok(())
}

async fn cmd_docker_tags(store: &ConfigStore, args: TagsArgs) -> Result<()> {
    let registry_name = store.resolve_registry_name(args.registry)?;
    let reg: RegistryConfig = store.load_effective_registry(&registry_name)?.config;

    let api = DockerApi::new(&reg)?;
    let (name, tags) = api.tags(&reg, &args.repository, args.n).await?;

    qprintln!("registry: {}", registry_name);
    qprintln!("repository: {}", name);
    for tag in tags {
        qprintln!("{}", tag);
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
    reg.base_path = normalize_base_path(&args.base_path);
    reg.crates.insecure_tls = args.insecure_tls;

    store.save_registry(scope, &args.name, &reg)?;

    if args.r#use || effective_root.crates.current_registry.is_none() {
        effective_root.crates.current_registry = Some(args.name.clone());
        store.save_root_config(scope, &effective_root)?;
    }

    ui::ok(format!("crates registry '{}' saved", args.name));
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
        ui::warn("no crates registries configured");
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

        let base_path = if entry.config.base_path.is_empty() {
            String::new()
        } else {
            format!(" base-path={}", entry.config.base_path)
        };

        qprintln!(
            "{} {} -> {}{}{} ({})",
            marker,
            entry.name,
            entry.config.crates.url,
            base_path,
            authed,
            source
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

    ui::ok(format!("active crates registry set to '{}'", args.name));
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

    ui::ok(format!("crates registry '{}' removed", args.name));
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
    ui::ok(format!(
        "token saved for crates registry '{}'",
        registry_name
    ));
    Ok(())
}

async fn cmd_crates_search(store: &ConfigStore, args: CratesSearchArgs) -> Result<()> {
    let registry_name = store.resolve_crates_registry_name(args.registry)?;
    let reg = store.load_effective_registry(&registry_name)?.config;

    let api = CratesApi::new(&reg.crates)?;
    let (crates, total) = api.search(&reg, &args.query, args.limit).await?;

    qprintln!("registry: {}", registry_name);
    qprintln!("query: \"{}\"  ({} total)", args.query, total);
    qprintln!();

    if crates.is_empty() {
        ui::warn("no crates found");
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
        qprintln!(
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
    let records = api.versions(&reg, &args.crate_name).await?;

    qprintln!("registry: {}", registry_name);
    qprintln!("crate: {}", args.crate_name);
    qprintln!();

    let to_show: Vec<_> = if args.all {
        records.iter().collect()
    } else {
        records.iter().filter(|r| !r.yanked).collect()
    };

    if to_show.is_empty() {
        if args.all {
            ui::warn("no versions found");
        } else {
            ui::warn("no active versions (use --all to include yanked)");
        }
        return Ok(());
    }

    // Header
    qprintln!("{:<20}  {:<8}  checksum", "version", "status");
    qprintln!("{}", "-".repeat(72));

    for r in to_show.iter().rev() {
        let status = if r.yanked { "yanked" } else { "active" };
        let short_cksum = if r.cksum.len() > 16 {
            format!("{}…", &r.cksum[..16])
        } else {
            r.cksum.clone()
        };
        qprintln!("{:<20}  {:<8}  {}", r.vers, status, short_cksum);
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
    api.yank(&reg, &args.crate_name, &args.version).await?;

    ui::ok(format!(
        "yanked {}-{} from '{}'",
        args.crate_name, args.version, registry_name
    ));
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
    api.unyank(&reg, &args.crate_name, &args.version).await?;

    ui::ok(format!(
        "unyanked {}-{} in '{}'",
        args.crate_name, args.version, registry_name
    ));
    Ok(())
}

// ---------------------------------------------------------------------------
// Files commands
// ---------------------------------------------------------------------------

async fn cmd_files_storages(store: &ConfigStore, args: FilesStoragesArgs) -> Result<()> {
    let registry_name = store.resolve_registry_name(args.registry)?;
    let registry = store.load_effective_registry(&registry_name)?.config;
    let api = FilesApi::new(&registry)?;
    let storages = api.storages(&registry).await?;

    qprintln!("registry: {}", registry_name);
    if storages.is_empty() {
        ui::warn("no storages configured");
        return Ok(());
    }

    for storage in storages {
        qprintln!("{} -> {}", storage.name, storage.root);
    }
    Ok(())
}

async fn cmd_files_ls(store: &ConfigStore, args: FilesLsArgs) -> Result<()> {
    let registry_name = store.resolve_registry_name(args.registry)?;
    let registry = store.load_effective_registry(&registry_name)?.config;
    let api = FilesApi::new(&registry)?;
    let result = api.list(&registry, &args.storage, &args.path).await?;

    qprintln!("registry: {}", registry_name);
    qprintln!("storage: {}", result.storage);
    qprintln!("path: /{}", result.path);
    qprintln!();

    if result.entries.is_empty() {
        qprintln!("(empty)");
        return Ok(());
    }

    for entry in result.entries {
        let kind = if entry.is_dir { "dir " } else { "file" };
        let size = if entry.is_dir {
            "-".to_string()
        } else {
            entry.size_bytes.to_string()
        };
        qprintln!("{} {:>10} {}", kind, size, entry.path);
    }
    Ok(())
}

async fn cmd_files_upload(store: &ConfigStore, args: FilesUploadArgs) -> Result<()> {
    if args.local_files.is_empty() {
        bail!("at least one local file is required");
    }

    let registry_name = store.resolve_registry_name(args.registry)?;
    let registry = store.load_effective_registry(&registry_name)?.config;
    let api = FilesApi::new(&registry)?;

    for local_file in &args.local_files {
        let bytes = std::fs::read(local_file)
            .map_err(|err| anyhow::anyhow!("failed to read {}: {}", local_file, err))?;
        let remote_path = remote_path_for_upload(local_file, args.remote_dir.as_deref())?;
        api.upload(&registry, &args.storage, &remote_path, bytes)
            .await?;
        ui::ok(format!("uploaded {} -> {}", local_file, remote_path));
    }

    Ok(())
}

async fn cmd_files_preview(store: &ConfigStore, args: FilesPreviewArgs) -> Result<()> {
    let registry_name = store.resolve_registry_name(args.registry)?;
    let registry = store.load_effective_registry(&registry_name)?.config;
    let api = FilesApi::new(&registry)?;
    let preview = api.preview(&registry, &args.storage, &args.path).await?;

    qprintln!("storage: {}", preview.storage);
    qprintln!("path: {}", preview.path);
    qprintln!("kind: {}", preview.kind);
    qprintln!("truncated: {}", preview.truncated);
    qprintln!();
    qprintln!("{}", preview.content);
    Ok(())
}

async fn cmd_files_download(store: &ConfigStore, args: FilesDownloadArgs) -> Result<()> {
    let registry_name = store.resolve_registry_name(args.registry)?;
    let registry = store.load_effective_registry(&registry_name)?.config;
    let api = FilesApi::new(&registry)?;
    let (bytes, server_name) = api.download(&registry, &args.storage, &args.path).await?;

    let output = args
        .output
        .or(server_name)
        .unwrap_or_else(|| "download.bin".to_string());
    std::fs::write(&output, bytes)?;
    ui::ok(format!("saved {}", output));
    Ok(())
}

async fn cmd_files_mkdir(store: &ConfigStore, args: FilesMkdirArgs) -> Result<()> {
    let registry_name = store.resolve_registry_name(args.registry)?;
    let registry = store.load_effective_registry(&registry_name)?.config;
    let api = FilesApi::new(&registry)?;
    api.mkdir(&registry, &args.storage, &args.path).await?;
    ui::ok(format!("folder created: {}", args.path));
    Ok(())
}

async fn cmd_files_rmdir(store: &ConfigStore, args: FilesRmdirArgs) -> Result<()> {
    let registry_name = store.resolve_registry_name(args.registry)?;
    let registry = store.load_effective_registry(&registry_name)?.config;
    let api = FilesApi::new(&registry)?;
    api.rmdir(&registry, &args.storage, &args.path).await?;
    ui::ok(format!("folder deleted: {}", args.path));
    Ok(())
}

async fn cmd_files_delete(store: &ConfigStore, args: FilesDeleteArgs) -> Result<()> {
    let registry_name = store.resolve_registry_name(args.registry)?;
    let registry = store.load_effective_registry(&registry_name)?.config;
    let api = FilesApi::new(&registry)?;
    api.delete_file(&registry, &args.storage, &args.path)
        .await?;
    ui::ok(format!("file deleted: {}", args.path));
    Ok(())
}

async fn cmd_files_bulk_delete(store: &ConfigStore, args: FilesBulkDeleteArgs) -> Result<()> {
    if args.paths.is_empty() {
        bail!("at least one path is required");
    }
    let registry_name = store.resolve_registry_name(args.registry)?;
    let registry = store.load_effective_registry(&registry_name)?.config;
    let api = FilesApi::new(&registry)?;
    api.bulk_delete(&registry, &args.storage, &args.paths)
        .await?;
    ui::ok("bulk delete complete");
    Ok(())
}

async fn cmd_files_bulk_download(store: &ConfigStore, args: FilesBulkDownloadArgs) -> Result<()> {
    if args.paths.is_empty() {
        bail!("at least one path is required");
    }
    let registry_name = store.resolve_registry_name(args.registry)?;
    let registry = store.load_effective_registry(&registry_name)?.config;
    let api = FilesApi::new(&registry)?;
    let bytes = api
        .bulk_download(&registry, &args.storage, &args.paths)
        .await?;
    std::fs::write(&args.output, bytes)?;
    ui::ok(format!("saved {}", args.output));
    Ok(())
}

// ---------------------------------------------------------------------------
// Admin commands
// ---------------------------------------------------------------------------

async fn run_admin(store: &ConfigStore, command: AdminCommands) -> Result<()> {
    match command {
        AdminCommands::Gc(args) => cmd_admin_gc(store, args).await,
    }
}

async fn cmd_admin_gc(store: &ConfigStore, args: AdminGcArgs) -> Result<()> {
    // Determine which registry to use
    let registry_name = store.resolve_registry_name(args.registry)?;
    let registry = store.load_effective_registry(&registry_name)?.config;

    let admin_api = AdminApi::new(&registry)?;

    ui::info(format!(
        "running garbage collection for registry '{}'",
        registry_name
    ));

    // Run Docker GC if requested or if no specific type was specified
    if args.docker || !args.crates {
        ui::info("running Docker garbage collection...");
        match admin_api.run_docker_gc(&registry, "/admin/docker/gc").await {
            Ok(report) => {
                ui::ok("Docker GC completed");
                qprintln!("  Deleted: {}", report.deleted);
                qprintln!("  Kept: {}", report.kept);
            }
            Err(e) => {
                ui::error(format!("Docker GC failed: {}", e));
                return Err(e);
            }
        }
    }

    // Run Crates GC if requested or if no specific type was specified
    if args.crates || !args.docker {
        ui::info("running crates garbage collection...");
        match admin_api.run_crates_gc(&registry, "/admin/crates/gc").await {
            Ok(report) => {
                ui::ok("Crates GC completed");
                qprintln!("  Deleted crates: {}", report.deleted_crates);
                qprintln!("  Kept crates: {}", report.kept_crates);
                qprintln!("  Removed index entries: {}", report.removed_index_entries);
                qprintln!("  Deleted owner files: {}", report.deleted_owner_files);
                qprintln!("  Removed empty dirs: {}", report.removed_empty_dirs);
            }
            Err(e) => {
                ui::error(format!("Crates GC failed: {}", e));
                return Err(e);
            }
        }
    }

    Ok(())
}
