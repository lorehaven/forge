//! The Android program of Forklift: the surface the host draws, and the background run (`--backup`). The backup's brain
//! itself (the controller, the platform it asks, the worker thread) is `forklift-host`, shared with the Linux host.

pub mod headless;

pub use forklift_host::*;
