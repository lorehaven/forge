# warehouse-core
What Warehouse's API says and how a form is read.

## What it is
`warehouse-core` has the management API's model, request bodies, drafts, registry trees and file tree. It does no I/O and builds for the browser, so the service and the client share it and its tests run natively and fast.

## Use cases
- Share one model between the service and the client.
- Check form input before it is sent.

## Documentation
Full documentation: [services/warehouse](../../../docs/services/warehouse/index.md)
