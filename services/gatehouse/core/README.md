# gatehouse-core
What Gatehouse's API says and how a form is read.

## What it is
`gatehouse-core` has the page API's wire model, redirect notices and form drafts. It does no I/O and builds for the browser, so the service and the client share it and its tests run natively and fast.

## Use cases
- Share one model between the service and the client.
- Check form input before it is sent.

## Documentation
Full documentation: [services/gatehouse](../../../docs/services/gatehouse/index.md)
