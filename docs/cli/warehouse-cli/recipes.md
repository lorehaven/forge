---
title: Recipes
parent: warehouse-cli
grand_parent: Command-line tools
nav_order: 5
---

The commands that contact a Warehouse are *Unverified* (not run for the book); registry setup was verified.

## Point the CLI at a Warehouse

```bash
warehouse docker registry add main --url https://host --base-path /warehouse --use
warehouse crates registry add main --url https://host --base-path /warehouse --use
warehouse files  registry add main --url https://host --base-path /warehouse --use
```

Repeat `--base-path` on every `add` of the same name (see the gotcha on [Registry commands](commands-registry.md#add)). Add `--global` to keep it outside the project directory.

## Browse images

```bash
warehouse docker login --username me --password '...'
warehouse docker catalog --n 200
warehouse docker tags forge/hoist-service
```

## Yank a bad release

```bash
warehouse crates login --token '...'
warehouse crates versions quench-args --all
warehouse crates yank quench-args 0.1.4
warehouse admin gc --crates              # remove yanked tarballs
```

## Move files

```bash
warehouse files storages
warehouse files upload backups db.sql.gz --remote-dir 2026/10
warehouse files ls backups --path 2026/10
warehouse files download backups 2026/10 --output october.zip   # a folder arrives as zip
```
