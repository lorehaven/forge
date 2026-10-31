# Daybook Service
Personal tasks and project lists API.

## What it is
`daybook` is a backend-only personal task API at `/daybook/api/v1`, with the browser client at `/daybook/ui`. Tasks without a `project_id` are in the user's inbox; custom projects provide Todoist-style lists. Gatehouse authenticates requests, and every database query is scoped to the JWT subject.

## Use cases
- Store tasks, lists, labels and repeats per user.
- Serve the browser client of the Daybook app.

## Documentation
Full documentation: [services/daybook](../../../docs/services/daybook/index.md)
