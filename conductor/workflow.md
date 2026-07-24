# Project Workflow

## Guiding Principles

1.  **The Plan is the Source of Truth:** All work must be tracked in `plan.md`
2.  **The Tech Stack is Deliberate:** Changes to the tech stack must be
    documented in `tech-stack.md` *before* implementation
3.  **Test-Driven Development:** Write unit tests before implementing
    functionality
4.  **High Code Coverage:** Aim for >80% code coverage for all modules
5.  **User Experience First:** Every decision should prioritize user experience
6.  **Non-Interactive & CI-Aware:** Prefer non-interactive commands. Use
    `CI=true` for watch-mode tools (tests, linters) to ensure single execution.
7.  **Parallel Execution Worktree/Target Isolation:** When running parallel
    subagents or concurrent test suites, each subagent MUST execute in an
    isolated git worktree or specify a unique `CARGO_TARGET_DIR` (e.g.,
    `CARGO_TARGET_DIR=target/<track_id>`) and isolated temp DB directory
    (e.g., `/tmp/<track_id>`) to eliminate build target lock contention and
    database lock conflicts.

## Task Workflow

All tasks follow a strict lifecycle:

### Standard Task Workflow

1.  **Select Task:** Choose the next available task from `plan.md` in sequential
    order

2.  **Mark In Progress:** Before beginning work, edit `plan.md` and change the
    task from `[ ]` to `[~]`

3.  **Write Failing Tests (Red Phase):**

    -   Create a new test file for the feature or bug fix.
    -   Write one or more unit tests that clearly define the expected behavior
        and acceptance criteria for the task.
    -   **CRITICAL:** Run the tests and confirm that they fail as expected. This
        is the "Red" phase of TDD. Do not proceed until you have failing tests.

4.  **Implement to Pass Tests (Green Phase):**

    -   Write the minimum amount of application code necessary to make the
        failing tests pass.
    -   Run the test suite again and confirm that all tests now pass. This is
        the "Green" phase.

5.  **Refactor (Obligatory):**

    -   With the safety of passing tests, refactor the implementation code and
        the test code to improve clarity, remove duplication, and enhance
        performance without changing the external behavior.
    -   Rerun tests to ensure they still pass after refactoring.

6.  **Verify Coverage:** Run coverage reports using the project's chosen tools.
    For example, in a Python project, this might look like: `bash pytest
    --cov=app --cov-report=html` Target: >80% coverage for new code. The
    specific tools and commands will vary by language and framework.

7.  **Document Deviations:** If implementation differs from tech stack:

    -   **STOP** implementation
    -   Update `tech-stack.md` with new design
    -   Add dated note explaining the change
    -   Resume implementation

8.  **Commit Code Changes:**

    -   Stage all code changes related to the task.
    -   Propose a clear, concise commit message e.g, `feat(ui): Create basic
        HTML structure for calculator`.
    -   Perform the commit.

9.  **Attach Task Summary with Git Notes:**

    -   **Step 9.1: Get Commit Hash:** Obtain the hash of the *just-completed
        commit* (`git log -1 --format="%H"`).
    -   **Step 9.2: Draft Note Content:** Create a detailed summary for the
        completed task. This should include the task name, a summary of changes,
        a list of all created/modified files, and the core "why" for the change.
    -   **Step 9.3: Attach Note:** Use the `git notes` command to attach the
        summary to the commit. `bash # The note content from the previous step
        is passed via the -m flag. git notes add -m "<note content>"
        <commit_hash>`

10. **Get and Record Task Commit SHA:**

    -   **Step 10.1: Update Plan:** Read `plan.md`, find the line for the
        completed task, update its status from `[~]` to `[x]`, and append the
        first 7 characters of the *just-completed commit's* commit hash.
    -   **Step 10.2: Write Plan:** Write the updated content back to `plan.md`.

11. **Commit Plan Update:**

    -   **Action:** Stage the modified `plan.md` file.
    -   **Action:** Commit this change with a descriptive message (e.g.,
        `conductor(plan): Mark task 'Create user model' as complete`).

### Task Correction & Plan Amendment Workflows

When an implemented task or phase requires corrections, amendments, or additions, follow these standard workflows to maintain plan integrity and avoid untracked code drift:

1.  **In-Flight Refinements:** If minor gaps are found while a task is actively
    in-progress (`[~]`), make the adjustments directly in the active
    implementation stream and ensure passing tests before committing.
2.  **Code Review Corrections (`conductor-review`):** If issues are identified
    during or after a code review, instruct the agent to review your changes
    (e.g., *"run a review"* or triggering the action manually in compatible
    clients). The review agent will automatically append a `Review Fixes` phase
    to `plan.md` so that correction tasks are formally tracked and
    checkpointed.
3.  **Logical State Reversions (`conductor-revert`):** If a task implementation
    is fundamentally flawed or needs to be redone, instruct the agent to revert
    the changes (e.g., *"revert the last task"* or triggering the action
    manually in compatible clients). This safely rolls back associated git
    commits and resets the task state in `plan.md` back to pending `[ ]` to
    allow a clean restart.

### Phase Completion Verification and Checkpointing Protocol

**Trigger:** This protocol is executed immediately after a task is completed
that also concludes a phase in `plan.md`.

1.  **Announce Protocol Start:** Inform the user that the phase is complete and
    the verification and checkpointing protocol has begun.

2.  **Ensure Test Coverage for Phase Changes:**

    -   **Step 2.1: Determine Phase Scope:** To identify the files changed in
        this phase, you must first find the starting point. Read `plan.md` to
        find the Git commit SHA of the *previous* phase's checkpoint. If no
        previous checkpoint exists, the scope is all changes since the first
        commit.
    -   **Step 2.2: List Changed Files:** Execute `git diff --name-only
        <previous_checkpoint_sha> HEAD` to get a precise list of all files
        modified during this phase.
    -   **Step 2.3: Verify and Create Tests:** For each file in the list:
        -   **CRITICAL:** First, check its extension. Exclude non-code files
            (e.g., `.json`, `.md`, `.yaml`).
        -   For each remaining code file, verify a corresponding test file
            exists.
        -   If a test file is missing, you **must** create one. Before writing
            the test, **first, analyze other test files in the repository to
            determine the correct naming convention and testing style.** The new
            tests **must** validate the functionality described in this phase's
            tasks (`plan.md`).

3.  **Execute Automated Tests with Proactive Debugging:**

    -   Before execution, you **must** announce the exact shell command you will
        use to run the tests.
    -   **Primary Command:** `MYTHRAX_TEST_MOCK=1 cargo nextest run`
    -   Execute the announced command.
    -   If tests fail, you **must** inform the user and begin debugging. You may
        attempt to propose a fix a **maximum of two times**. If the tests still
        fail after your second proposed fix, you **must stop**, report the
        persistent failure, and ask the user for guidance.

4.  **Execute dev50 Regression Benchmark Gate:**

    -   **CRITICAL:** This step is mandatory for every phase completion. It
        verifies that the phase's changes did not regress the memory engine's
        retrieval quality or latency.
    -   **Command:** `bash scripts/verify_dev50.sh`
    -   This runs the dev50 benchmark split in hybrid mode against the baseline
        (`bench_data/BASELINE_DEV50.json`), checking:
        -   **Recall_Any@5** — must not regress below baseline
        -   **Recall_All@5** — must not regress below baseline
        -   **nDCG@10** — must not regress below baseline
        -   **Avg Latency** — must not exceed 115% of baseline
    -   Results are logged to `bench_data/dev50_history.jsonl` and
        `bench_data/dev50_state.json`.
    -   **Gate:** If the script exits with status `REJECT`, the phase **cannot**
        be committed or checkpointed. You must debug the regression, fix it, re-run
        the unit tests (Step 3), and re-run dev50 until it passes.
    -   **Announce Result:** Report the dev50 metrics and PASS/REJECT status to
        the user before proceeding.

5.  **Propose a Detailed, Actionable Manual Verification Plan:**

    -   **CRITICAL:** To generate the plan, first analyze `product.md`,
        `product-guidelines.md`, and `plan.md` to determine the user-facing
        goals of the completed phase.
    -   You **must** generate a step-by-step plan that walks the user through
        the verification process, including any necessary commands and specific,
        expected outcomes.
    -   For backend/engine changes, include commands to monitor daemon RSS via
        `footprint` or `ps aux`, and verify VRAM usage stability during model
        inference.

6.  **Await Explicit User Feedback:**

    -   After presenting the detailed plan, ask the user for confirmation:
        "**Does this meet your expectations? Please confirm with yes or provide
        feedback on what needs to be changed.**"
    -   **PAUSE** and await the user's response. Do not proceed without an
        explicit yes or confirmation.

7.  **Run Conductor Code Review (Principal Engineer):**

    -   **Action:** Immediately after the user provides confirmation, execute
        the `conductor-review` skill to perform a standard code review of the
        phase's implementation.
    -   **Persona:** The Conductor Reviewer acts as a meticulous **Principal
        Software Engineer** and **Code Review Architect**. It is helpful but
        firm in its standards.
    -   **Scope:** The reviewer must:
        -   Verify the code implements what `plan.md` and `spec.md` asked for
            (intent verification).
        -   Check strict compliance with `product-guidelines.md` and any
            `conductor/code_styleguides/*.md` files.
        -   Scan for bugs, race conditions, null pointer risks, hardcoded
            secrets, PII leaks, and unsafe input handling.
        -   Verify new tests exist and cover the changes.
        -   Run the test suite automatically and analyze results.
    -   **Gate:** If the Conductor Reviewer identifies issues, they must be
        resolved before proceeding to the CTO review. The reviewer may
        auto-apply fixes with user approval, or the user may fix manually.
    -   **Commit Fixes:** If fixes are applied, they must be committed and
        tracked in `plan.md` under a "Review Fixes" phase before proceeding.

8.  **Run Adversarial CTO Code Review (Fix-Resubmit Loop):**

    -   **Action:** After the Conductor Review passes, invoke a `cto_reviewer`
        subagent to perform an adversarial code review of the phase's
        implementation. This is the final, most hostile gate.
    -   **Persona:** The CTO Reviewer acts as a hostile, skeptical Principal
        Software Engineer. It thinks from first principles, challenges every
        assumption, and prioritizes correctness and safety over speed.
    -   **Scope:** The reviewer must:
        -   Read the phase's `plan.md` tasks and the `spec.md` requirements.
        -   Independently audit the changed source files (`git diff` from
            previous checkpoint) for bugs, memory leaks, race conditions,
            missing edge cases, and incomplete fixes.
        -   Verify no code/test gaps exist, no edges are missing, and no
            technical debt was bypassed.
        -   Check that proposed tests are sufficiently strict to fail if the
            underlying business logic is stubbed or bypassed.
        -   Look for systemic patterns the implementer may have missed (e.g.,
            similar bugs in files not covered by the phase).
    -   **Output:** The reviewer must produce a structured report with **ALL**
        findings categorized as Critical, High, Medium, or Low, with exact file
        paths, line numbers, and suggested fixes. The report must conclude with
        an explicit verdict: either `APPROVED` (unconditional) or
        `CHANGES REQUESTED` (with the full findings list).
    -   **Fix-Resubmit Loop:** If the CTO Reviewer returns `CHANGES REQUESTED`
        with **any** findings at **any** severity level (Critical, High,
        Medium, or Low), the coding agent must:
        1.  Implement fixes for **every** finding in the report.
        2.  Re-run unit tests (Step 3).
        3.  Re-run dev50 regression benchmark (Step 4).
        4.  Re-invoke the CTO Reviewer subagent with the updated diff.
        5.  Repeat this loop until the CTO Reviewer returns `APPROVED` with
            zero findings remaining.
    -   **Gate:** The phase cannot be committed or checkpointed until the CTO
        Reviewer grants **unconditional `APPROVED`** status with no outstanding
        findings. There is no override for this gate.

9.  **Conditional Commit — All Gates Must Pass:**

    -   **Precondition:** The phase may only be committed if ALL of the
        following gates have passed:
        -   Unit tests (Step 3): PASS
        -   dev50 regression benchmark (Step 4): PASS
        -   User manual verification (Step 6): Confirmed
        -   Conductor Review (Step 7): No unresolved issues
        -   Adversarial CTO Review (Step 8): Unconditional `APPROVED` verdict
    -   **If all gates pass:** Stage all code changes and commit with a scoped
        message (e.g., `fix(memory): Phase N — <description>`).
    -   **If any gate fails:** Do NOT commit. Debug, fix, and re-run the
        failing gate(s) until all pass.

10. **Identify Target Commit for Report:**

    -   The target commit is the commit created in Step 9.
    -   Obtain its hash via `git log -1 --format="%H"`.

11. **Attach Auditable Verification Report using Git Notes:**

    -   **Step 11.1: Draft Note Content:** Create a detailed verification report
        including: the automated test command and result, the dev50 benchmark
        metrics (Recall@5, nDCG@10, latency), the manual verification steps,
        the user's confirmation, the Conductor Review summary, and the CTO
        Review summary.
    -   **Step 11.2: Attach Note:** Use the `git notes` command to attach the
        full report to the target commit identified in Step 10.

12. **Get and Record Phase Checkpoint SHA:**

    -   **Step 12.1: Get Commit Hash:** Obtain the hash of the commit from
        Step 9 (`git log -1 --format="%H"`).
    -   **Step 12.2: Update Plan:** Read `plan.md`, find the heading for the
        completed phase, and append the first 7 characters of the commit hash in
        the format `[checkpoint: <sha>]`.
    -   **Step 12.3: Write Plan:** Write the updated content back to `plan.md`.

13. **Commit Plan Update:**

    -   **Action:** Stage the modified `plan.md` file.
    -   **Action:** Commit this change with a descriptive message following the
        format `conductor(plan): Mark phase '<PHASE NAME>' as complete`.

14. **Announce Completion:** Inform the user that the phase is complete and the
    checkpoint has been created, with the detailed verification report attached
    as a git note. Include the dev50 metrics delta from baseline in the
    announcement.

### Quality Gates

Before marking any task complete, verify:

-   [ ] All tests pass
-   [ ] Code coverage meets requirements (>80%)
-   [ ] Code follows project's code style guidelines (as defined in
    `code_styleguides/`)
-   [ ] All public functions/methods are documented (e.g., docstrings, JSDoc,
    GoDoc)
-   [ ] Type safety is enforced (e.g., type hints, TypeScript types, Go types)
-   [ ] No linting or static analysis errors (using the project's configured
    tools)
-   [ ] Works correctly on mobile (if applicable)
-   [ ] Documentation updated if needed
-   [ ] No security vulnerabilities introduced

## Development Commands

**AI AGENT INSTRUCTION: This section should be adapted to the project's specific
language, framework, and build tools.**

### Setup

```bash
# Example: Commands to set up the development environment (e.g., install dependencies, configure database)
# e.g., for a Node.js project: npm install
# e.g., for a Go project: go mod tidy
```

### Daily Development

```bash
# Example: Commands for common daily tasks (e.g., start dev server, run tests, lint, format)
# e.g., for a Node.js project: npm run dev, npm test, npm run lint
# e.g., for a Go project: go run main.go, go test ./..., go fmt ./...
```

### Before Committing

```bash
# Example: Commands to run all pre-commit checks (e.g., format, lint, type check, run tests)
# e.g., for a Node.js project: npm run check
# e.g., for a Go project: make check (if a Makefile exists)
```

## Testing Requirements

### Unit Testing

-   Every module must have corresponding tests.
-   Use appropriate test setup/teardown mechanisms (e.g., fixtures,
    beforeEach/afterEach).
-   Mock external dependencies.
-   Test both success and failure cases.

### Integration Testing

-   Test complete user flows
-   Verify database transactions
-   Test authentication and authorization
-   Check form submissions

### Mobile Testing

-   Test on actual iPhone when possible
-   Use Safari developer tools
-   Test touch interactions
-   Verify responsive layouts
-   Check performance on 3G/4G

## Code Review Process

### Self-Review Checklist

Before requesting review:

1.  **Functionality**

    -   Feature works as specified
    -   Edge cases handled
    -   Error messages are user-friendly

2.  **Code Quality**

    -   Follows style guide
    -   DRY principle applied
    -   Clear variable/function names
    -   Appropriate comments

3.  **Testing**

    -   Unit tests comprehensive
    -   Integration tests pass
    -   Coverage adequate (>80%)

4.  **Security**

    -   No hardcoded secrets
    -   Input validation present
    -   SQL injection prevented
    -   XSS protection in place

5.  **Performance**

    -   Database queries optimized
    -   Images optimized
    -   Caching implemented where needed

6.  **Mobile Experience**

    -   Touch targets adequate (44x44px)
    -   Text readable without zooming
    -   Performance acceptable on mobile
    -   Interactions feel native

## Commit Guidelines

### Message Format

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

### Types

-   `feat`: New feature
-   `fix`: Bug fix
-   `docs`: Documentation only
-   `style`: Formatting, missing semicolons, etc.
-   `refactor`: Code change that neither fixes a bug nor adds a feature
-   `test`: Adding missing tests
-   `chore`: Maintenance tasks

### Examples

```bash
git commit -m "feat(auth): Add remember me functionality"
git commit -m "fix(posts): Correct excerpt generation for short posts"
git commit -m "test(comments): Add tests for emoji reaction limits"
git commit -m "style(mobile): Improve button touch targets"
```

## Definition of Done

A task is complete when:

1.  All code implemented to specification
2.  Unit tests written and passing
3.  Code coverage meets project requirements
4.  Documentation complete (if applicable)
5.  Code passes all configured linting and static analysis checks
6.  Works beautifully on mobile (if applicable)
7.  Implementation notes added to `plan.md`
8.  Changes committed with proper message
9.  Git note with task summary attached to the commit

## Emergency Procedures

### Critical Bug in Production

1.  Create hotfix branch from main
2.  Write failing test for bug
3.  Implement minimal fix
4.  Test thoroughly including mobile
5.  Deploy immediately
6.  Document in plan.md

### Data Loss

1.  Stop all write operations
2.  Restore from latest backup
3.  Verify data integrity
4.  Document incident
5.  Update backup procedures

### Security Breach

1.  Rotate all secrets immediately
2.  Review access logs
3.  Patch vulnerability
4.  Notify affected users (if any)
5.  Document and update security procedures

## Deployment Workflow

### Pre-Deployment Checklist

-   [ ] All tests passing
-   [ ] Coverage >80%
-   [ ] No linting errors
-   [ ] Mobile testing complete
-   [ ] Environment variables configured
-   [ ] Database migrations ready
-   [ ] Backup created

### Deployment Steps

1.  Merge feature branch to main
2.  Tag release with version
3.  Push to deployment service
4.  Run database migrations
5.  Verify deployment
6.  Test critical paths
7.  Monitor for errors

### Post-Deployment

1.  Monitor analytics
2.  Check error logs
3.  Gather user feedback
4.  Plan next iteration

## Continuous Improvement

-   Review workflow weekly
-   Update based on pain points
-   Document lessons learned
-   Optimize for user happiness
-   Keep things simple and maintainable
