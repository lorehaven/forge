# Dev certificate
The one TLS certificate and key every service uses when the estate runs locally.

## What it is
`foreman.toml` points every service's `SERVER_CERT_PATH` and `SERVER_KEY_PATH` at `certs/cert.pem` and `certs/key.pem`, so the whole estate speaks HTTPS with the same certificate and a Secure cookie set by one service is readable by the next. The files are not committed (`*.pem` is ignored): put a certificate and its key here. Without them the services fall back to plain HTTP.

The certificate is issued for `registry.local`, not `localhost`, which is why the estate sets `GATEHOUSE_TLS_VERIFY=false`.

## Documentation
Full documentation: [running locally](../docs/guides/running-locally.md)
