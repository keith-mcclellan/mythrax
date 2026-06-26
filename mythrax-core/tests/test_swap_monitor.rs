use mythrax_core::daemon::monitor::{check_disk_space, check_memory_pressure, check_swap_pressure};
use mythrax_core::llm::ModelTier;
use tempfile::tempdir;

#[test]
fn test_canonicalized_mount_point_disk_check() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let target_dir = temp_dir.path().join("download_dir");
    std::fs::create_dir_all(&target_dir).unwrap();

    let symlink_dir = temp_dir.path().join("symlinked_dir");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_dir, &symlink_dir).unwrap();

    let massive_bytes = 10 * 1024 * 1024 * 1024 * 1024; // 10 Terabytes
    let res = check_disk_space(&symlink_dir, massive_bytes);
    assert!(res.is_err(), "Must correctly canonicalize symlink and fail disk space check on partition");
}

#[test]
fn test_model_aware_swap_eviction_thresholds() {
    // Tier 1: 1.5B (Threshold 2.0 GB)
    let evict_tier1_high = check_swap_pressure(ModelTier::Tier1, 2_100 * 1024 * 1024);
    assert!(evict_tier1_high, "Tier 1 must evict at 2.1 GB swap");
    let evict_tier1_low = check_swap_pressure(ModelTier::Tier1, 1_500 * 1024 * 1024);
    assert!(!evict_tier1_low, "Tier 1 must not evict at 1.5 GB swap");

    // Tier 2: 7B Coder (Threshold 3.0 GB)
    let evict_tier2_high = check_swap_pressure(ModelTier::Tier2, 3_100 * 1024 * 1024);
    assert!(evict_tier2_high, "Tier 2 must evict at 3.1 GB swap");
    let evict_tier2_low = check_swap_pressure(ModelTier::Tier2, 2_500 * 1024 * 1024);
    assert!(!evict_tier2_low, "Tier 2 must not evict at 2.5 GB swap");

    // Tier 3: 35B Deep Reason (Threshold 6.0 GB)
    let evict_tier3_high = check_swap_pressure(ModelTier::Tier3, 6_100 * 1024 * 1024);
    assert!(evict_tier3_high, "Tier 3 must evict at 6.1 GB swap");
    let evict_tier3_low = check_swap_pressure(ModelTier::Tier3, 5_500 * 1024 * 1024);
    assert!(!evict_tier3_low, "Tier 3 must not evict at 5.5 GB swap");
}
