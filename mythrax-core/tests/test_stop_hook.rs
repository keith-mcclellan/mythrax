use mythrax_core::hooks::stop::should_save;

#[test]
fn cadence_triggers_every_15_human_messages() {
    // Cadence triggers when we cross a multiple of 15 (e.g. 14 -> 15, 29 -> 30)
    assert!(should_save(14, 15), "Should trigger at 15");
    assert!(!should_save(15, 16), "Should not trigger at 16");
    assert!(!should_save(0, 5), "Should not trigger at 5");
    assert!(should_save(29, 30), "Should trigger at 30");
    assert!(!should_save(30, 31), "Should not trigger at 31");
    assert!(should_save(44, 45), "Should trigger at 45");
}
