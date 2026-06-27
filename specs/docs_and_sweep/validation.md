# Validation: Documentation Update & Background Transcript Sweep

## Acceptance Criteria Review
*All acceptance criteria must be checked and validated after execution.*

- [ ] Overwritten `ARCHITECTURE.md` contains accurate step-by-step data flows for Bootstrapping, Watchers, completions proxies, model swaps, and compactors.
- [ ] Overwritten `DEVELOPMENT.md` contains the correct individual test commands for all these data flow steps.
- [ ] Stashing `_transcript_path` in STM registers the path successfully.
- [ ] Background dreaming coordinator queries STM for `_transcript_path` and mines idle sessions (>10 mins inactivity).
- [ ] STM cleanup deletes the `_transcript_path` key upon successful sweep completion.
- [ ] Integration tests in `test_compactor.rs` verify that an idle session is successfully swept and compacted.

## Test Results
*Test outputs and statuses will be filled in during the validation phase.*

## Final Status
*Pending Execution*
