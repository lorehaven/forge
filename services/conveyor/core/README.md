# conveyor-core
What Conveyor's API says and how a form is read.

## What it is
`conveyor-core` has the records, wire models, drafts, project tree, job graph and formatting. It does no I/O and builds for the browser, so the service and the client share it and its tests run natively and fast.

## Use cases
- Share one model between the service and the client.
- Check form input before it is sent.

## Documentation
Full documentation: [services/conveyor](../../../docs/services/conveyor/index.md)
