# workbench-core
What Workbench's API says and how a form is read.

## What it is
`workbench-core` has the model, request bodies, drafts and the board. It does no I/O and builds for the browser, so the service and the client share it and its tests run natively and fast.

## Use cases
- Share one model between the service and the client.
- Check form input before it is sent.

## Documentation
Full documentation: [services/workbench](../../../docs/services/workbench/index.md)
