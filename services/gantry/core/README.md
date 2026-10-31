# gantry-core
What Gantry's API says and how a form is read.

## What it is
`gantry-core` has the model, request bodies, filters and the YAML draft. It does no I/O and builds for the browser, so the service and the client share it and its tests run natively and fast.

## Use cases
- Share one model between the service and the client.
- Check form input before it is sent.

## Documentation
Full documentation: [services/gantry](../../../docs/services/gantry/index.md)
