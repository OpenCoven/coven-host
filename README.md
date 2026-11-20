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

The MVP supports a single client, peer and Castle. Creates and updates propagate in both directions. A deletion is reported for resolution and the peer keeps its copy.

**What a Castle is.** A Castle is a bounded, hosted place where a Project's work runs and its results come back, with exactly one execution owner. Coven Host provides a Castle's folder surface: `Castles/<name>/files/` in, `Castles/<name>/results/` out. [coven-castles](https://github.com/OpenCoven/coven-castles) checks whether a sandbox is fit to back a Castle, and Coven Compass shows Castles inside Projects. A Castle is not a Project, so its folder path and `castle.json` aren't a Project identity. Coven Host, coven-castles and Coven Compass share this definition.

## Preview the Host landing page

Open [the landing page](docs/index.html) from a local checkout, or run `python3 -m http.server --directory docs 8000` and visit [localhost](http://localhost:8000/).
