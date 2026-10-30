# Daybook Service

Daybook is a backend-only personal task API. Tasks without a `project_id` are
in the user's inbox; custom projects provide Todoist-style lists. Gatehouse
authenticates requests, and every database query is scoped to the JWT subject.

See [the service documentation](../../../docs/services/daybook/index.md).
