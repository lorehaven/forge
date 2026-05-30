use serde::Serialize;
use std::process::Command;

#[derive(Debug, Default, Serialize)]
pub struct GpuInfo {
    pub name: String,
    pub total_gb: f64,
    pub used_gb: f64,
    pub free_gb: f64,
}

pub fn get_gpu_info() -> std::io::Result<GpuInfo> {
    let output = Command::new("rocm-smi")
        .args(["--showmeminfo", "vram", "--json"])
        .output()?;

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;

    let card = json.as_object().unwrap().values().next().unwrap();

    let total: f64 = card["VRAM Total Memory (B)"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let used: f64 = card["VRAM Total Used Memory (B)"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let total_gb = total / 1024.0f64.powi(3);
    let used_gb = used / 1024.0f64.powi(3);

    Ok(GpuInfo {
        name: "card0".into(),
        total_gb: round2(total_gb),
        used_gb: round2(used_gb),
        free_gb: round2(total_gb - used_gb),
    })
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
