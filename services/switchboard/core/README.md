# switchboard-core
What Switchboard's API says and how a form is read.

## What it is
`switchboard-core` has the model, request bodies, fit arithmetic and filters. It does no I/O and builds for the browser, so the service and the client share it and its tests run natively and fast.

## Use cases
- Share one model between the service and the client.
- Check form input before it is sent.

## Documentation
Full documentation: [services/switchboard](../../../docs/services/switchboard/index.md)
