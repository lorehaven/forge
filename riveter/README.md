# Riveter 🏗️

Riveter is a specialized tool for managing Kubernetes manifests, powered by Rust and the `minijinja` templating engine. It simplifies the process of rendering, applying, and managing resource definitions across different environments.

## Features

- **Template Rendering**: Uses Jinja2-style templates (via `minijinja`) to generate Kubernetes YAML manifests.
- **Environment Management**: Easily switch between and manage different environments (e.g., dev, staging, prod).
- **Kubectl Integration**: Direct support for `apply` and `delete` operations via `kubectl`.
- **Dry-run Support**: Preview changes before applying them to the cluster.
- **Interactive REPL**: A built-in REPL for quick environment management and resource operations.

## Installation

```bash
cd build-tools/riveter
cargo install --path .
```

## Usage

### Environment Management

- `riveter env list`: List all available environments.
- `riveter env set <env>`: Set the current active environment.
- `riveter env show`: Show the currently active environment.

### Manifest Operations

- `riveter render`: Render templates for the current environment.
- `riveter apply [--dry-run]`: Apply the rendered manifests to the Kubernetes cluster.
- `riveter delete`: Delete the resources defined in the manifests from the cluster.

### Interactive REPL

Simply run `riveter` or `riveter repl` to enter the interactive shell:

```bash
riveter
```

## Templates

Riveter expects templates to be located in `src/templates` within the module (or as configured). Templates use the `.yaml.j2` extension.

## License
MIT
