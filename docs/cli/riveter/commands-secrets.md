---
title: Secrets
parent: riveter
grand_parent: Command-line tools
nav_order: 3
---

`riveter secrets ...` keeps an overlay's secrets in git, encrypted with [age](https://age-encryption.org), and opens them at render/install time. `sync` is the older route that puts `.env` files into the cluster. Verified locally with a throwaway key: `keygen`, `set`, `list`, `show`, error paths. Unverified (not run: touches a cluster): `sync` without `--dry-run`.

## File format

A secrets file (`secrets.yaml` in an overlay directory; the name `secrets.yaml` is the one packages and installs look for) is YAML that is safe to commit:

```yaml
# Encrypted with age: committed, and opened only with the key named by RIVETER_AGE_KEY or RIVETER_AGE_KEY_FILE.
# Change it with `riveter secrets set|import|rekey`, not by hand.
riveter-secrets: 1
recipients:
- age1…
data:
  DB_PASSWORD: ENC[age,<base64 ciphertext>]
```

Every value must be `ENC[age,...]`; a plaintext value is rejected (`` `NAME` is not encrypted (a value must be ENC[age,...]); add it with `riveter secrets set` ``). `data` is sorted so the file is stable. Version marker `riveter-secrets: 1`.

The key is looked up as: `RIVETER_AGE_KEY` (the secret key text), else `RIVETER_AGE_KEY_FILE` (a file holding it; an explicit path that is not a file is an error), else `~/.config/riveter/age.key` if it exists. For these two variable names only, a `.env` in the working directory is consulted after the process environment. The variables inside the file become `${NAME}` values for the overlay; without a key: `this overlay has encrypted values (secrets.yaml) but no key to open them: set RIVETER_AGE_KEY, or RIVETER_AGE_KEY_FILE to a key file`.

## keygen

```text
riveter secrets keygen [--out <FILE>]
```

Makes an age key pair. With `--out`, writes the private key file (mode `0600`, created exclusively: `<file> already exists; refusing to overwrite a key`) and prints `ok private key written to <file> (keep it out of git)`. Without it the key text is printed. Either way the public recipient (`age1...`) is the last line printed. The file holds `# created: <rfc3339>`, `# public key: <age1...>` and the secret key.

```text
$ riveter secrets keygen --out k.txt
ok private key written to k.txt (keep it out of git)
age1fm95aw9cueuefas5flq0suqp54vehzeypw0hjyzz9aff7xnee92sn3mv2s
```

## set

```text
riveter secrets set <FILE> <NAME> [--value <VALUE>] [-r, --recipient <AGE1...>]...
```

Encrypts one value into the file, leaving the others as they were. The value comes from `--value` or, if omitted, from standard input (one trailing newline is dropped; empty is an error: `no value on standard input (or pass --value)`), which keeps it out of shell history. A new file needs recipients: `--recipient`, or a configured key to derive them from (`a new file needs recipients: ...`). An existing file keeps its own recipients; naming others fails (`<file> already encrypts to other recipients; use riveter secrets rekey to change them`). Prints `ok <NAME> encrypted into <file>`.

## import

```text
riveter secrets import <FILE> --from <DOTENV> [NAME]... [-r, --recipient <AGE1...>]...
```

Encrypts values from a dotenv file into the secrets file. With names, only those (`` `NAME` is not in <dotenv> `` if absent); without, all, sorted. Prints `ok N value(s) encrypted into <file>: A, B`.

## list

```text
riveter secrets list <FILE>        alias: ls
```

Prints the names, never the values. Needs no key.

## show

```text
riveter secrets show <FILE> <NAME>
```

Prints one decrypted value to standard output. Needs the key (`no key to open it: set RIVETER_AGE_KEY to the key, or RIVETER_AGE_KEY_FILE to a file holding it`).

## remove

```text
riveter secrets remove <FILE> <NAME>
```

Drops a name (`` `NAME` is not in <file> `` if absent). Prints `ok <NAME> removed from <file>`.

## rekey

```text
riveter secrets rekey <FILE> -r <AGE1...> [-r ...]
```

`--recipient`/`-r` is required, repeatable. Decrypts every value with the configured key and re-encrypts to the new recipients, replacing the old. Prints `ok N value(s) in <file> now encrypted to M recipient(s)`.

## sync

```text
riveter secrets sync [OVERLAY]... [--all] [-n, --namespace <NS>] [--context <CTX>] [--dry-run]
```

Writes each named overlay's `.env` to a Secret `gantry-values-<overlay>` holding the file as it is under the key `env`, using your own kubectl access (values go to kubectl on standard input and are never printed). `--all` takes every overlay with a `.env`. Defaults: namespace `forge`, current kubectl context. `--dry-run` reports what would be synced and touches no cluster. Prints `ok synced <secret> (<ns>): N variable(s): A, B` (`would sync` with `--dry-run`, then `warn dry run: nothing was written to the cluster`). Without any overlay `.env`: `no overlay under overlays has a .env to sync` (verified).
