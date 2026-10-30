---
title: Observability
parent: Sage
grand_parent: Services
nav_order: 11
---

Sources: `service/src/observability/*`, `runtime/rate_limiter.rs`, `tools/mod.rs`. All state is in process memory and lost on restart; nothing is per replica shared.

## Metrics

`MetricsCollector` records every tool execution (profile, tool, duration, success, timeout). `GET /api/v1/chat/metrics` returns `{"profiles": [ProfileMetrics...]}`, `GET /api/v1/chat/metrics/{profile}` one profile (`404` `api_error_metrics_not_found` before its first call). `ProfileMetrics`: `profile`, `total_calls`, `total_successes`, `total_failures`, `tools` (map name to `ToolMetrics`: `calls`, `successes`, `failures`, `timeouts`, `total_duration_ms`, `min_duration_ms`, `max_duration_ms`). Derived values (success rate, average duration) are methods, not fields.

## Costs

`CostTracker` records one `RequestCost` per tool execution (user, conversation, profile, tool cost with tokens, API calls, duration). Estimated cost = tokens * 0.000002 + API calls * 0.001 (hard-coded example rates). `GET /api/v1/chat/costs` returns `{"users": [UserCosts], "profiles": [ProfileCosts]}`; `GET /api/v1/chat/costs/user/{user_id}` one user (`404` `api_error_costs_not_found`). Fields: `request_count`, `total_duration_ms`, `total_tokens_used`, `total_api_calls`, `estimated_cost`, `avg_cost_per_request`. These are not restricted to the caller.

## Rate limiting

`RateLimiter` runs inside the tool registry (not on HTTP routes), per profile: a sliding one-minute window per user (`calls_per_minute`, error `UserRateLimitExceeded`) and a per-conversation burst (`burst_limit` calls in the last minute, error `BurstLimitExceeded`). Defaults: web_assistant 120/30, code_assistant 60/15, cli_agent 30/5, other profiles 60/15. A refused call returns the tool result `Rate limit exceeded: <error>` and an audit warning. The limits cannot be configured by environment variable (`set_config` exists in code only). The `command` tool has its own limit of 5 calls per second.

## Audit log

`AuditLogger` writes `quench_log` events with `target: "audit"` for tool executions, restricted-tool attempts (a tool outside the profile), confirmation-required refusals and rate-limit hits.

## Context status

`GET /api/v1/chat/context-status/{profile}` reports the limits of the profile with 0 current tokens: 8192 (warn at 75 percent) for `code_assistant` and `cli_agent`, 4096 (80 percent) otherwise. Turns do not use these limits; they size the prompt from the instance's `max_model_len` ([Events](events.md)).

## Clients

`SwitchboardClient` (circuit breaker 5 failures / 3 successes / 60 s; 3 attempts 100 ms to 2 s) and `VllmClient` (circuit breaker 3 / 2 / 60 s) keep request metrics (`RequestMetrics`) that are not exposed over HTTP.
