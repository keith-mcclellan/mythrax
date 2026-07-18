cd mythrax-core
export MYTHRAX_TEST_MOCK=1
time cargo test cognitive::synthesis::tests::test_dbscan_cosine_metrics -- --test-threads=1
