# daybook-app
The app crate of Daybook.

## What it is
`daybook-app` is Daybook's screens. The app does no I/O: it asks its host for things (`Request`) and is told what happened (`Msg`), so the same screens run on every host and in tests. Daybook is about tasks, lists, labels and repeats.

## Use cases
- Used by the other `daybook-*` crates; see the layout in the [Daybook README](../README.md).

## Documentation
Full documentation: [services/daybook](../../../docs/services/daybook/index.md)
