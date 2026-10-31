# Switchboard Service
Models registry and vLLM instance manager.

## What it is
`switchboard-service` holds the registry of models and starts, stops and sizes the vLLM instances that serve them, with a launch form that checks the fit against the GPU.

## Use cases
- Register a model and launch it on a GPU.
- See what runs where.

## Documentation
Full documentation: [services/switchboard](../../../docs/services/switchboard/index.md)
