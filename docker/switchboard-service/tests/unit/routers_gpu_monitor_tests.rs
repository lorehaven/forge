//! Reading the GPU's video memory from sysfs, against a stand-in directory.

use std::path::{Path, PathBuf};

use switchboard_service::routers::gpu::monitor::read_gpu_info;

const GIB: u64 = 1024 * 1024 * 1024;

fn sysfs(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("switchboard-drm-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn card(root: &Path, name: &str, total: u64, used: u64) {
    let device = root.join(name).join("device");
    std::fs::create_dir_all(&device).unwrap();
    std::fs::write(device.join("mem_info_vram_total"), format!("{total}\n")).unwrap();
    std::fs::write(device.join("mem_info_vram_used"), format!("{used}\n")).unwrap();
}

#[quench_test::test]
fn the_card_reports_its_total_used_and_free_memory() {
    let root = sysfs("one");
    card(&root, "card0", 32 * GIB, 8 * GIB);
    let info = read_gpu_info(&root).unwrap();
    assert_eq!(info.name, "card0");
    assert!((info.total_gb - 32.0).abs() < 1e-9);
    assert!((info.used_gb - 8.0).abs() < 1e-9);
    assert!((info.free_gb - 24.0).abs() < 1e-9);
    let _ = std::fs::remove_dir_all(&root);
}

#[quench_test::test]
fn the_biggest_card_is_the_one_models_run_on_not_an_integrated_one() {
    let root = sysfs("two");
    card(&root, "card0", GIB / 2, GIB / 4);
    card(&root, "card1", 24 * GIB, 2 * GIB);
    // The connectors under a card are not cards, and a directory with no counters is skipped.
    std::fs::create_dir_all(root.join("card1-DP-1")).unwrap();
    std::fs::create_dir_all(root.join("card2/device")).unwrap();
    std::fs::create_dir_all(root.join("renderD128")).unwrap();
    let info = read_gpu_info(&root).unwrap();
    assert_eq!(info.name, "card1");
    assert!((info.total_gb - 24.0).abs() < 1e-9);
    let _ = std::fs::remove_dir_all(&root);
}

#[quench_test::test]
fn equal_cards_resolve_to_the_first() {
    let root = sysfs("tie");
    card(&root, "card1", 16 * GIB, 0);
    card(&root, "card0", 16 * GIB, 4 * GIB);
    assert_eq!(read_gpu_info(&root).unwrap().name, "card0");
    let _ = std::fs::remove_dir_all(&root);
}

#[quench_test::test]
fn without_a_gpu_it_is_an_error_the_callers_turn_into_zeros() {
    let root = sysfs("none");
    std::fs::create_dir_all(root.join("card0/device")).unwrap();
    std::fs::write(
        root.join("card0/device/mem_info_vram_total"),
        "not a number",
    )
    .unwrap();
    assert_eq!(
        read_gpu_info(&root).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    assert!(read_gpu_info(&root.join("missing")).is_err());
    let _ = std::fs::remove_dir_all(&root);
}
