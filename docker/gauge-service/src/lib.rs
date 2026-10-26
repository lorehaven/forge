//! Gauge - the estate's host metrics dashboard.
//!
//! A gauge is the dial on a workshop machine that says how hard it is working. This service scrapes the
//! PCP (`pmproxy`) and `node_exporter` endpoints of the hosts it is told about, keeps a short history in
//! memory and shows CPU, memory, GPU, disks and network per host.

// Router `Result<T, Response>` early-returns trip this; boxing buys nothing.
#![allow(clippy::result_large_err)]
// Metric maths and drawing: values are small and rounded for display, so these casts and fused
// multiply-adds are not worth their noise.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    clippy::too_many_lines
)]

pub mod domain;
pub mod routers;
