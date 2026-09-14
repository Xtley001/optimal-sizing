# Setup, Git Push, Run, and Publish

One walkthrough covering everything needed to take this unzipped project from a local folder to a working GitHub repo, a passing local build, and (when ready) a published crate.

## 1. Prerequisites

- Rust via [rustup](https://rustup.rs), stable channel.
- No other system dependencies. `sizing-core` and `sizing-bench` need no network access; `sizing-integration` needs outbound HTTPS to whatever RPC endpoint you point it at.
- Optional: [GitHub CLI](https://cli.github.com) (`gh`) for a one-command repo push.

## 2. Push to GitHub

You're starting from an unzipped folder — there's no `.git` directory yet.

```bash
cd optimal-sizing
git init
git add .
git status   # sanity check: target/ should NOT appear (.gitignore excludes it)
git commit -m "Initial commit: optimal-sizing"
```

If `target/` (or anything build-artifact-shaped) shows up in `git status`, stop and confirm `.gitignore` is present at the repo root first — a committed `target/` directory will bloat the repo by hundreds of MB.

`Cargo.lock` **is** meant to be committed — this workspace ships several binaries, so a reproducible lockfile is worth more here than in a pure-library crate.

**Create and push the remote:**

```bash
# Option A — GitHub CLI
gh repo create optimal-sizing --public --source=. --remote=origin --push

# Option B — web UI
# 1. Create an empty repo at github.com/new (no README/license/.gitignore — you already have them)
# 2. Then:
git branch -M main
git remote add origin https://github.com/<your-username>/optimal-sizing.git
git push -u origin main
```

Before your first push, check that `crates/sizing-core/Cargo.toml`'s `authors`/`repository` fields and the `LICENSE` attribution match who you actually are — a find-and-replace now is cheaper than doing it after it's baked into commit history.

**Confirm CI is green:** pushing to `main` (or opening a PR) triggers [`.github/workflows/ci.yml`](./.github/workflows/ci.yml) — build, test, clippy (`-D warnings`), and a `cargo tree` check that `sizing-core` has picked up no chain/network dependency. Check the **Actions** tab after your first push.

**Turn on GitHub Pages** (one manual step, can't be scripted): repo → **Settings** → **Pages** → under "Build and deployment" set Source to **GitHub Actions** (not "Deploy from a branch"). [`.github/workflows/docs.yml`](./.github/workflows/docs.yml) then deploys `cargo doc` output to `https://<you>.github.io/optimal-sizing/` on every push to `main`.

## 3. Build, test, run locally

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

All three should pass clean — this is exactly what CI runs on every push/PR. See [`docs/TESTING.md`](./docs/TESTING.md) for the per-suite command list and testing contract.

**Benchmarks** (regenerates [`docs/benchmark-report.md`](./docs/benchmark-report.md) with freshly measured numbers):

```bash
cargo run -p sizing-bench --release
```

**Live reference integration** against real Ethereum mainnet pool state:

```bash
RPC_URL=<your Ethereum mainnet RPC endpoint> cargo run -p sizing-integration
```

`RPC_URL` is required; the program exits with a clear error if it's unset and never falls back to hardcoded data. Before trusting its output, see [`SECURITY.md`](./SECURITY.md)'s "Known unverified assumptions" — in particular, verify Curve 3pool's `A()` scaling against Etherscan and supply `CURVE_A_OVERRIDE` if needed.

**Docs, locally:**

```bash
cargo doc --workspace --no-deps --open
```

### Toolchain note

If you're on an older or non-`rustup` toolchain and hit a version-resolution error, two dependency pins in `Cargo.toml` files across the workspace exist to accommodate that (`proptest`'s exact version, and a few dev-dependency patch pins that predate certain crates' `edition2024` requirement). On a normal current stable toolchain (1.85+) you can typically delete those pinned lines and run `cargo update` — they exist for compatibility, not because a newer version is unsafe.

## 4. Publishing to crates.io

Only `sizing-core` is meant to be published — it's the actual library. `sizing-bench`, `sizing-integration`, and `sizing-cli` are internal tooling with a path dependency on it and carry no publish metadata. `sizing-router` is a genuine library extension that could reasonably be published alongside `sizing-core` in the future but isn't publish-configured yet. `sizing-wasm`/`sizing-py` belong to their own ecosystems' registries (`wasm-pack publish`, `maturin publish` to PyPI) rather than crates.io.

**One-time setup:**

1. Create a [crates.io](https://crates.io) account (GitHub OAuth works).
2. Get an API token: crates.io → Account Settings → API Tokens → New Token.
3. `cargo login <your-token>` — or, to avoid the token touching disk in a throwaway environment (e.g. a Codespace), set it as the `CARGO_REGISTRY_TOKEN` environment variable instead; `cargo` picks that up automatically with no `cargo login` step.

**Before your first publish**, confirm `crates/sizing-core/Cargo.toml`'s `authors`/`repository` fields and `LICENSE` match your identity, and that `description`/`keywords`/`categories` reflect how you want the crate to show up in crates.io search.

**Publish:**

```bash
cd crates/sizing-core

# Dry run first — catches packaging errors without publishing
cargo package --allow-dirty

# Inspect exactly what would be uploaded
cargo package --list

# When satisfied — this is permanent (you can `cargo yank` a version later, not delete it)
cargo publish
```

`sizing-core`'s only dependencies are `rust_decimal`/`rust_decimal_macros`, both already on crates.io, so there's no publish-ordering concern.

**Versioning:** currently `0.1.0` — pre-1.0, so breaking changes may land in `0.x` minor bumps per semver convention. Bump to `1.0.0` once you're ready to commit to the public API in `docs/API.md` long-term.

## 5. Developing in GitHub Codespaces (optional)

The repo ships [`.devcontainer/devcontainer.json`](./.devcontainer/devcontainer.json), so **Code → Codespaces → Create codespace on main** gives you a fully provisioned Rust toolchain (`rust-analyzer`, `clippy`, `rustfmt`) with the workspace already built once via `postCreateCommand` — no manual setup.

- **Running `sizing-integration` with a real RPC endpoint:** don't paste the URL into the terminal directly. Add it as a Codespaces secret instead (your avatar → **Settings** → **Codespaces** → **Codespaces secrets** → New secret, name it `RPC_URL`), restart the Codespace to pick it up, then `cargo run -p sizing-integration` as usual. Same pattern for `CURVE_A_OVERRIDE`, `UNISWAP_REFERENCE_PRICE`, `CURVE_REFERENCE_PRICE` if you want them to persist, or just `export` them for a one-off run.
- **Publishing from a Codespace:** store your crates.io token as a `CARGO_REGISTRY_TOKEN` secret the same way, so it's never written to the container's local credentials file where it'd be lost when the Codespace is deleted.
- **Viewing rustdoc output in-browser:** `--open` can't launch a local browser from inside a Codespace — right-click `target/doc/sizing_core/index.html` in the file explorer and choose **Show Preview** instead.

## 6. Quick reference

| You want to... | See |
|---|---|
| Understand the public API | [`docs/API.md`](./docs/API.md) |
| Understand the math | [`docs/whitepaper.md`](./docs/whitepaper.md) |
| Module boundaries / non-goals | [`docs/ARCHITECTURE.md`](./docs/ARCHITECTURE.md) |
| Testing contract | [`docs/TESTING.md`](./docs/TESTING.md) |
| What's safe to trust for real money | [`SECURITY.md`](./SECURITY.md) |
| What's built vs. still proposed | [`CHANGELOG.md`](./CHANGELOG.md) / [`ROADMAP.md`](./ROADMAP.md) |
| Contribution guidelines | [`CONTRIBUTING.md`](./CONTRIBUTING.md) |
