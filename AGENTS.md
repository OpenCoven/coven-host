# Working on Coven Host

Start with the [delivery plan](docs/plan.md) for current work and its next owner. The [operator guide](docs/operations.md) describes behavior that existing folders, databases and peer implementations rely on.

The product is Rust. The Linux compatibility tests extract the historical Python prototype at a fixed commit into a temporary directory. Keep product changes in Rust and use that fixture to verify interoperability and upgrades.

For uploaded documents, read the [storage and processing decision](docs/uploaded-documents.md) before changing the peer or proposing a daemon processor. The [operator guide](docs/operations.md) describes the released behavior.

## Enable the commit guard

Install Gitleaks 8.30.1 or a compatible v8 release and run the following once per clone. Git shares this configuration with linked worktrees. If the clone already uses a custom hooks directory, call this repository’s `.githooks/pre-commit` from the existing pre-commit hook to preserve its other hooks.

```sh
gitleaks version
git config --local core.hooksPath .githooks
```

Git runs the guard against the staged changes on commit. It uses Gitleaks’ default rules plus checks for Coven Host tokens, local environment files, binary keystores and sync databases. A missing scanner or a finding blocks the commit. To check the index manually, run `git hook run pre-commit`. After changing the guard, run `scripts/check.sh hook` to exercise blocked and allowed commits in a disposable repository. If it flags a credential, remove it from the index and keep it in protected storage. If it was already shared, revoke or rotate it before cleaning up the published copy.

## Find and claim work

1. Check [open issues](https://github.com/OpenCoven/coven-host/issues?q=is%3Aissue%20is%3Aopen) and [open pull requests](https://github.com/OpenCoven/coven-host/pulls) for an existing owner, overlapping paths, dependencies and the last handoff. For cross-repository changes, check [Coven’s integration issue](https://github.com/OpenCoven/coven/issues/1805) as well.
2. Record the outcome, owned paths, verification and dependencies on the tracking issue or existing PR. Use its assignee or a dated handoff to identify the active owner. Coordinate overlapping changes there before editing a shared migration, schema or protocol.
3. Fetch the current refs and inspect `git worktree list` and `git status --short --branch`. Use a separate branch and worktree for independent work. Preserve other worktrees and uncommitted files.
4. At handoff, record the exact revision, checks run, first remaining failure or untested platform, and the next action and owner. Update the plan when a milestone or dependency changes. Use the PR for day-to-day review status.

## Source and verification map

| Change | Implementation | Focused verification |
| --- | --- | --- |
| Castle names, path exclusions and local folder quotas | `src/lib.rs`, `src/folder.rs` | `cargo test --locked --test path_policy` checks that a full Castle leaves its sibling usable |
| Peer HTTP, authentication, revisions and retry receipts | `src/peer.rs`, `src/peer/server.rs` | `cargo test --locked --test peer` and Linux compatibility matrix |
| Daemon profile ownership at initialization and reopen | `src/client.rs`, `src/folder.rs` | `cargo test --locked --test client profile` checks that exactly one profile wins, its owner can reopen, and legacy adoption requires explicit selection. Run `cargo test --locked --test peer profile_refusal` to verify refusal preserves both profiles’ SQLite state, user bytes and retained copies and that the owner reconciles with its original peer |
| Offline state, SQLite schema and conflict resolution | `src/client.rs`, `src/client/schema.sql`, `src/client/reconcile.rs` | `cargo test --locked --test client` and stopped-Python upgrade |
| macOS token-file permissions and recovery | `src/folder.rs`, `src/folder/macos_acl.rs` | On APFS run `scripts/check-macos-token.sh` or pass the absolute path to an installed Coven CLI to exercise its production daemon. |
| Atomic replacement, retained versions and filesystem boundaries | `src/folder.rs`, `src/folder/{unix,windows,windows_security,previous}.rs` | `cargo test --locked --test folder`, library unit tests and affected native filesystem |
| Linux compatibility and upgrades | `tests/compat-linux.sh`, `examples/compat_client.rs` | `scripts/check.sh compat`, which runs `tests/test_integration.py` and `tests/test_upgrade.py` |
| Checks and CI | `scripts/check.sh`, `.github/workflows/ci.yml` | `scripts/check.sh --fast` and the pull request checks |
| Desktop installation, login services and setup diagnostics | `installer/macos/`, `installer/windows/`, `installer/linux/` | [Build and qualify the desktop installers](installer/README.md) |
| Native command dispatch, daemon API and installed packages | Separate Coven repository, including `crates/coven-cli/src/host_cli.rs`, `crates/coven-cli/src/host_sync.rs` and `crates/coven-client/src/host.rs` | Coven workspace tests and installed CLI with the production daemon |

Run `scripts/check.sh` before handing off; it runs the [verification commands](docs/acceptance.md#run-the-checks) that CI runs on every pull request. Use `scripts/check.sh --full` on Linux for the mixed-language matrix. Run file replacement and recovery changes on the affected volume type. For wire or schema changes, include retry after lost acknowledgement, host rollback and Python-created state. Use disposable roots, storage and tokens. Keep credentials and private recovery paths out of shared reports.

For a two-client change, run `scripts/check.sh --full` on Linux. The native adapter exercises both reconnect orders, both conflict choices and a lost acknowledgement during that matrix. Repeat with `python3 tests/test_multi_client.py "$INSTALLED_COVEN"`, using the absolute path to the installed CLI and a temporary directory on the filesystem under test. Use `python3 tests/installed_profile_recovery.py "$INSTALLED_COVEN"` to verify ownership before migration and recovery against the installed daemon.

For named Castles, run the focused Rust suites and the installed two-Castle recovery journey below. The journey creates temporary profiles, drops a committed acknowledgement, restarts both endpoints and checks both recovery copies and revisions. Pass the actual candidate installation's CLI path.

```sh
cargo test --locked --test path_policy --test peer --test client --test folder
python3 tests/test_named_castles.py /absolute/path/to/installed/coven
python3 tests/test_multi_client.py /absolute/path/to/installed/coven
```

For public CLI qualification, install the registry version under test in a fresh npm prefix. Record both wrapper and native package digests, the installed executable digest, and `coven --version`. Use that prefix’s executable for the two recovery commands above. Check the production daemon with a disposable profile. [Installation channels](docs/setup.md#which-cli-supports-the-folder) · [Operator walkthrough](docs/operations.md#add-a-named-castle) · [Registry evidence](docs/acceptance.md#public-npm-cli)

For physical SSH qualification, use the [operator walkthrough](docs/operations.md#verify-recovery-across-an-ssh-disconnect), compare the [observed revisions and recovery hashes](docs/acceptance.md#ssh-walkthrough), and record which hardware or authorized access is missing with the [issue owner in the delivery plan](docs/plan.md#what-can-i-pick-up-next).

## Publishing a handoff

Link the tracking issue and identify any consumer pin separately from the PR head. Record whether evidence came from source tests, a packaged binary, a real daemon or a remote peer. For Markdown, keep each ordinary paragraph on one source line and inspect the final rendered body. Use separate list items, table rows and code fences where they carry meaning.
