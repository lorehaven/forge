//! What Gauge's API says and how it is read: the model and its rates, the numbers as people read them, the chart geometry.

pub mod api;
pub mod chart;
pub mod forecast;
pub mod format;
pub mod model;

pub use api::{AlertEvent, DiskForecast, HostCard, HostDetail, HostHistory, HostSummary, Range};
pub use format::Level;
pub use model::{
    Alert, DiskStat, Fan, Fault, Filesystem, Gpu, LinkView, MemParts, Point, PsiPct, Rate, Reading,
    Temp, View,
};
