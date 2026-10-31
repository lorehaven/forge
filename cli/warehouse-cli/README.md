# warehouse-cli
Command-line client of the Warehouse storage service.

## What it is
`warehouse-cli` installs the `warehouse` binary: a client for the Docker registry, the Cargo registry, file storage and admin functions of Warehouse. A named registry file holds the URLs and credentials of one Warehouse for all three kinds.

## Use cases
- List and inspect Docker repositories and tags.
- Search, yank and unyank crates.
- Upload, download and browse files in a storage.
- Run registry garbage collection (`warehouse admin gc`).

## Example
```sh
warehouse files storages
warehouse files upload backup.tar.zst
warehouse crates search quench
```

## Documentation
Full documentation: [cli/warehouse-cli](../../docs/cli/warehouse-cli/index.md)
