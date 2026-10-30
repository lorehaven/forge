---
title: Configuration
parent: warehouse-cli
grand_parent: Command-line tools
nav_order: 4
---

The CLI has no environment variables and no config flags besides `--global` on the registry/login commands. All state is TOML files. Verified by running the registry commands against a scratch directory and `HOME`.

## Layout

```text
.warehouse/                      local scope (current directory), the default
  config.toml                    active registry per kind
  registries/<name>.toml         one file per registry
~/.config/warehouse/             global scope (--global), needs $HOME
  config.toml
  registries/<name>.toml
```

`$HOME` unset: `--global` fails with `HOME is not set; cannot use global config`, and global files are ignored on read.

Resolution rules:

* **Registry file** `<name>`: the local file wins over the global one; `registry list` shows the winner's scope.
* **Active registry** per kind: the local `config.toml` value wins, else the global one (merged per kind).
* Writes go to the scope chosen by `--global` (default local); `registry use` requires the registry file to exist in that scope.
* Directories are created on demand (`failed to create .warehouse/registries`).

## config.toml

```toml
[docker]
current_registry = "main"

[crates]
current_registry = "main"

[files]
current_registry = "main"
```

All keys optional (`Option<String>`).

## registries/\<name\>.toml

```toml
base_path = ""                       # shared by all kinds; normalised ("/warehouse")

[docker]
url = "http://registry.local:8698"   # required when the table exists
path = "/v2"                         # default "/v2"
service = "registry.local:8698"      # optional
insecure_tls = false                 # default false
username = "u"                       # optional, plain text (docker login)
password = "..."                     # optional, plain text

[crates]
url = "https://reg.example.com"
token = "..."                        # optional, plain text (crates login)
insecure_tls = false

[files]
url = "http://f.local"
insecure_tls = false
```

Every table and `base_path` default when absent (`#[serde(default)]`); a missing `url` inside an existing table is a parse error. Unknown keys are ignored. Files are written with `to_string_pretty` and are not made private: credentials are stored in plain text with the process's default file mode (observed `0644`); keep `.warehouse/` out of git (add it to `.gitignore`) or use `--global`.

## URL building

* Docker: `<docker.url><docker.path>/<endpoint>`.
* Crates, files, admin: `<url><base_path>/<endpoint>`, where `<url>` is the kind's URL (admin: crates URL, else docker URL; files: files, else docker, else crates URL).
* TLS verification can be disabled per kind with `insecure_tls`; the files client is insecure if either the docker or the crates table says so; the admin client likewise.
