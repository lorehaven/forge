# sage-core
What Sage's API says and how a form is read.

## What it is
`sage-core` has the wire model and the rules the browser client applies to it. It does no I/O and builds for the browser, so the service and the client share it and its tests run natively and fast.

## Use cases
- Share one model between the service and the client.
- Check form input before it is sent.

## Documentation
Full documentation: [services/sage](../../../docs/services/sage/index.md)
