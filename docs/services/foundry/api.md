---
title: API
parent: Foundry
grand_parent: Services
nav_order: 1
---

Foundry has no HTTP API. It binds no port, registers no routes and serves no health endpoint. [openapi.yaml](openapi.yaml) is a valid OpenAPI 3.1 document with `paths: {}`; it exists only so the documentation checker treats every service uniformly.

The interface of foundry is its command line: see [CLI](cli.md).
