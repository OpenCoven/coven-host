# Working on Coven Host

Start with the [delivery plan](docs/plan.md) for current work and its next owner. The [operator guide](docs/operations.md) describes behavior that existing folders, databases and peer implementations rely on.

The product is Rust. The Linux compatibility tests extract the historical Python prototype at a fixed commit into a temporary directory. Keep product changes in Rust and use that fixture to verify interoperability and upgrades.

## Enable the commit guard

Install Gitleaks 8.30.1 or a compatible v8 release and run the following once per clone. Git shares this configuration with linked worktrees. If the clone already uses a custom hooks directory, call this repository’s `.githooks/pre-commit` from the existing pre-commit hook to preserve its other hooks.

```sh
gitleaks version
git config --local core.hooksPath .githooks
```

Git runs the guard against the staged changes on commit. It uses Gitleaks’ default rules plus checks for Coven Host tokens, local environment files, binary keystores and sync databases. A missing scanner or a finding blocks the commit. To check the index manually, run `git hook run pre-commit`. After changing the guard, run `./tests/pre-commit.sh` to exercise blocked and allowed commits in a disposable repository. If it flags a credential, remove it from the index and keep it in protected storage. If it was already shared, revoke or rotate it before cleaning up the published copy.

## Find and claim work

1. Check [open issues](https://github.com/OpenCoven/coven-host/issues?q=is%3Aissue%20is%3Aopen) and [open pull requests](https://github.com/OpenCoven/coven-host/pulls) for an existing owner, overlapping paths, dependencies and the last handoff. For cross-repository changes, check [Coven’s integration issue](https://github.com/OpenCoven/coven/issues/1805) as well.
2. Record the outcome, owned paths, verification and dependencies on the tracking issue or existing PR. Use its assignee or a dated handoff to identify the active owner. Coordinate overlapping changes there before editing a shared migration, schema or protocol.
3. Fetch the current refs and inspect `git worktree list` and `git status --short --branch`. Use a separate branch and worktree for independent work. Preserve other worktrees and uncommitted files.
4. At handoff, record the exact revision, checks run, first remaining failure or untested platform, and the next action and owner. Update the plan when a milestone or dependency changes. Use the PR for day-to-day review status.

## Source and verification map

| Change | Implementation | Focused verification |
| --- | --- | --- |
| Path grammar, exclusions and quotas | `src/lib.rs` | `cargo test --locked --test path_policy` |
| Peer HTTP, authentication, revisions and retry receipts | `src/peer.rs`, `src/peer/server.rs` | `cargo test --locked --test peer` and Linux compatibility matrix |
| Offline state, SQLite schema and conflict resolution | `src/client.rs`, `src/client/schema.sql`, `src/client/reconcile.rs` | `cargo test --locked --test client` and stopped-Python upgrade |
| Atomic replacement, retained versions and filesystem boundaries | `src/folder.rs`, `src/folder/{unix,windows,windows_security,previous}.rs` | `cargo test --locked --test folder`, library unit tests and affected native filesystem |
| Linux compatibility and upgrades | `tests/compat-linux.sh`, `examples/compat_client.rs` | `tests/test_integration.py`, `tests/test_upgrade.py` |
| Native command dispatch, daemon API and installed packages | Separate Coven repository, including `crates/coven-cli/src/host_cli.rs`, `crates/coven-cli/src/host_sync.rs` and `crates/coven-client/src/host.rs` | Coven workspace tests and installed CLI with the production daemon |

Use the [verification commands](docs/acceptance.md#run-the-checks) for the full suite and mixed-language matrix. Run file replacement and recovery changes on the affected volume type. For wire or schema changes, include retry after lost acknowledgement, host rollback and Python-created state. Use disposable roots, storage and tokens. Keep credentials and private recovery paths out of shared reports.

## Publishing a handoff

Link the tracking issue and identify any consumer pin separately from the PR head. Record whether evidence came from source tests, a packaged binary, a real daemon or a remote peer. For Markdown, keep each ordinary paragraph on one source line and inspect the final rendered body. Use separate list items, table rows and code fences where they carry meaning.
