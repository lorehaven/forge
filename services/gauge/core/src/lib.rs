//! What Gauge's API says and how it is read: the model and its rates, the numbers as people read them, the chart geometry.

pub mod api;
pub mod chart;
pub mod format;
pub mod model;

pub use api::{HostCard, HostDetail, HostSummary};
pub use format::Level;
pub use model::{Filesystem, Gpu, Point, Rate, Reading, Temp, View};
