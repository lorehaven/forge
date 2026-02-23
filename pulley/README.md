# Pulley

An interactive REPL-based backup tool using rsync, configured via TOML files.

## Features

- **Interactive REPL interface** - Manage backups through an interactive command-line interface
- **TOML configuration** - Simple and readable configuration format
- **Multiple backup jobs** - Configure and manage multiple backup jobs
- **Dry-run mode** - Preview changes before applying them
- **Local and remote backups** - Support for both local paths and remote sources via SSH
- **Directory filtering** - Skip specific directories during backup
- **Deletion sync** - Optionally delete extraneous files from destination
- **Confirmation prompts** - Optional confirmation before executing updates

## Prerequisites

- Rust (stable)
- rsync
- SSH (for remote sources)

## Installation

Build the project:
```bash
cargo build --release -p pulley
```

## Configuration

Pulley uses a multi-file configuration system supporting multiple global and local config files:

- **Global**: `~/.config/pulley/*.toml` - All `.toml` files in this directory
- **Local**: `*.pulley.toml` - All `.pulley.toml` files in current directory

### Configuration Merging

1. All global configs are loaded and merged alphabetically
2. All local configs are loaded and merged alphabetically (overriding globals)
3. Jobs with matching IDs are overwritten by later configs
4. Jobs with unique IDs are appended to the list

This allows organizing jobs by purpose across multiple files.

### Example Global Configuration

Create organized config files in `~/.config/pulley/`:

**`~/.config/pulley/personal.toml`:**
```toml
[[jobs]]
id = "documents"
desc = "Backup documents folder"
src = "/home/user/Documents"
dest = "/mnt/backup/documents"
delete = true
skip = ["temp", "cache"]
no-confirm = false

[[jobs]]
id = "photos"
desc = "Backup personal photos"
src = "/home/user/Photos"
dest = "/mnt/backup/photos"
delete = false
no-confirm = true
```

**`~/.config/pulley/servers.toml`:**
```toml
[[jobs]]
id = "web-server"
desc = "Backup web server content"
src = "user@webserver.com:/var/www"
dest = "/mnt/backup/webserver"
delete = false
skip = ["logs", "cache"]
no-confirm = true
```

### Example Local Configuration

Create project-specific configs in your project directory:

**`project.pulley.toml`:**
```toml
# Override the global 'documents' job for this project
[[jobs]]
id = "documents"
desc = "Backup project documentation"
src = "./docs"
dest = "/mnt/backup/myproject-docs"
delete = false
no-confirm = true

# Add project-specific jobs
[[jobs]]
id = "source-code"
desc = "Backup project source code"
src = "./src"
dest = "/mnt/backup/myproject-src"
delete = true
skip = ["target", "node_modules", ".git"]
no-confirm = false
```

**`database.pulley.toml`:**
```toml
[[jobs]]
id = "db-backup"
desc = "Backup project database"
src = "./backups/db"
dest = "/mnt/backup/myproject-db"
delete = true
no-confirm = true
```

### Configuration Options

- `id` - Unique identifier for the job
- `desc` - Human-readable description
- `src` - Source path (local or remote with `user@host:/path` format)
- `dest` - Destination path (local)
- `delete` - Delete extraneous files from destination (default: false)
- `skip` - List of directory names to skip (default: [])
- `no-confirm` - Skip confirmation prompt (default: false)

## Usage

Run the REPL:
```bash
cargo run -p pulley
```

### REPL Commands

- `list` - List all configured jobs
- `run <job_id> [job_id2...]` - Run specific job(s) by ID
- `run all` - Run all configured jobs
- `reload` - Reload configuration file
- `help` - Show available commands
- `quit` or `exit` - Exit the REPL

### Example Session

```
$ cargo run -p pulley
Loading configuration from: pulley.config.toml
Configuration loaded successfully. 2 job(s) found.

Pulley Backup REPL
Type 'help' for available commands

pulley> list
Configured jobs:
  documents - Backup documents folder
    src: /home/user/Documents
    dest: /mnt/backup/documents
    delete: true
    skip: temp, cache

  photos - Backup photos from remote server
    src: user@server.com:/home/user/photos
    dest: /mnt/backup/photos
    skip: thumbnails
    no-confirm: true

pulley> run documents
Jobs to be run: documents

Starting job: `Backup documents folder`
Executing dry-run - listing files to update
>> `work`: 5 updatable or missing files
>> `personal`: 2 updatable or missing files
`Backup documents folder`: 7 total updatable or missing files
Continue? (y/n): y
Executing update
>> `work`: updating ...
>> `personal`: updating ...
Done job: `Backup documents folder`

pulley> quit
Goodbye!
```

## Configuration Priority

1. **Global configs** (`~/.config/pulley/*.toml`) are loaded first in alphabetical order
2. **Local configs** (`*.pulley.toml`) are loaded second in alphabetical order
3. Later configs override earlier configs when job IDs match
4. All config files are optional, but at least one must contain jobs

## Configuration Organization Tips

**Global configs** - Organize by category:
- `personal.toml` - Personal files and documents
- `servers.toml` - Remote server backups
- `work.toml` - Work-related backups
- `media.toml` - Photos, videos, music

**Local configs** - Organize by project needs:
- `project.pulley.toml` - Main project files
- `database.pulley.toml` - Database backups
- `assets.pulley.toml` - Large asset files
- `override.pulley.toml` - Temporary overrides (alphabetically last)

## Comparison with Backup Project

Pulley is a reimplementation of the backup project with the following enhancements:

- **REPL interface** instead of CLI-only operation
- **TOML configuration** instead of YAML
- **Interactive job management** - list, run, and reload jobs without restarting
- **Better error handling** with thiserror
- Same rsync functionality and features

## License

MIT
