# Coven Host

Coven Host keeps an ordinary folder in sync with one authenticated peer. Edit files offline under `Castles/demo/files/`, receive artifacts under `Castles/demo/results/`, and reconnect to reconcile. When both sides edit a file, you can inspect both versions and choose which bytes to keep.

The Rust library in this repository implements file transfer, reconciliation and the loopback peer service. The native `coven host` commands, daemon and npm packaging live in [OpenCoven/coven](https://github.com/OpenCoven/coven/issues/1805).

| I want to… | Start here |
| --- | --- |
| Install Coven Host | [Choose your desktop installer](docs/setup.md) |
| Connect a folder, run the peer or recover a file | [Native operator guide](docs/operations.md) |
| Find the next task, its owner and its completion condition | [MVP delivery plan](docs/plan.md) |
| Enable the commit guard | [Contributor setup](AGENTS.md#enable-the-commit-guard) |
| Find source, tests and related work before editing | [Agent and contributor guide](AGENTS.md) |
| Check which binaries, platforms and behaviors passed | [Acceptance report and verification commands](docs/acceptance.md) |

The MVP supports a single client, peer and Castle. When both endpoints support deletion, edits, renames and deletions synchronize in either direction. If one side deletes a file while the other edits it, the CLI reports where to recover the edited bytes and lets you choose the outcome. [Recover a deletion or rename](docs/operations.md#recover-a-deletion-or-rename)

## Preview the Host landing page

Open [the landing page](docs/index.html) from a local checkout, or run `python3 -m http.server --directory docs 8000` and visit [localhost](http://localhost:8000/).
