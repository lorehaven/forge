# Running Gantry locally

Postgres and Redis are real; the cluster and the runner are simulated in memory, so nothing here can touch a
Kubernetes cluster. The simulated cluster starts from `seed.json`: package `ml` at 1.0.0 with inference
(Sage, Switchboard) **up** and training (the trainer) **down**, and `media` with one workload. `ml` and `media`
each have a 1.1.0 in `packages/`, so there is something to upgrade to.

```text
cargo build -p gantry-service -p foundry-service      # workspace root
docker/gantry-service/local/pack.sh                   # needs riveter on PATH (or RIVETER=...)
cd docker/gantry-service/local && docker compose up
```

Open <http://localhost:11443/gantry/ui/home>. One card per package; click one for its resources, filterable by kind, state and name. Things to try:

- **Start training.** Under `ml`, the *training* deployment is stopped; click *Start*. Inference (Sage and
  Switchboard) is stopped first and its vLLM pods deleted, then training is applied from the package - one
  operation, no preview. Watch it from the banner or *History*. Swap back with *Start* on inference.
- **Stop is delete.** Sage's Deployment disappears from the list's live side and shows as `missing`; *Apply*
  brings it back from the package, because Gantry remembers what the package declares.
- **Edit** a ConfigMap: change the YAML, *Apply*. The row turns `edited` until the next upgrade overwrites it.
- **Upgrade** `ml` while training is stopped: training stays stopped.
- Restart with `docker compose restart gantry`: the simulated cluster returns to `seed.json`, but operations,
  recorded states and the inventory stay in Postgres, so rows can show as drifted until you
  `docker compose down -v` for a clean start.

What this proves is the screens, the recording and the logic. It does not run `riveter` or `kubectl`;
`GANTRY_RUNNER=local` against a scratch namespace (see `docs/docker/gantry-service.md`) does that. Packages are
rebuilt by `pack.sh`; `*.rivet` files are not committed.
