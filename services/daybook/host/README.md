# daybook-host
The host crate of Daybook.

## What it is
`daybook-host` is Daybook's controller the hosts share, over a `Platform` trait: changes show at once and are undone if refused. It touches the device only through `Platform`, so it runs the same under a test. Daybook is about tasks, lists, labels and repeats.

## Use cases
- Used by the other `daybook-*` crates; see the layout in the [Daybook README](../README.md).

## Documentation
Full documentation: [services/daybook](../../../docs/services/daybook/index.md)
