# genevia

Family trees: import a GEDCOM file, then browse it as a list (Ahnentafel table by generation), a pedigree or a radial fan,
and edit people, couples and relatives. Docs: [`docs/services/genevia`](../../../docs/services/genevia/index.md).

```text
foreman start genevia      postgres, the schema (foundry), the service on https://localhost:17443/genevia/ui
foreman run web            build the browser client into service/web/app.wasm (before the first start)
```

The trees are the `genevia` schema in Postgres (Foundry module `genevia`). Access is `genevia:read` / `genevia:write`,
for every tree or for one (`genevia:tree:<id>:read|write`); see the docs.
