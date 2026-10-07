# neoherdr

This checkout is **neoherdr**, a fork of [herdr](https://github.com/herdrdev/herdr)
(remote `danctorres/neoherdr`, not the canonical `herdrdev/herdr`) with a
different design philosophy: keyboard-first instead of mouse-first, every
action reachable through a discoverable prefix/which-key keymap, mouse kept
as a secondary path. The config format and wire protocol stay
`herdr`-compatible. The installed command is `neoherdr` and release builds
keep config, state, and sockets under `neoherdr` (for example
`~/.config/neoherdr`), so the fork runs beside an upstream herdr install.

Subcommands, `HERDR_*` environment variables, and help text keep upstream's
`herdr` spelling; `--help` opens with a line saying to type `neoherdr`.
Panes get a `herdr` link to this binary appended to `PATH`
(`<config dir>/bin`), so agents that call `herdr` work without upstream
installed. POSIX SSH remotes receive `~/.local/bin/neoherdr`. Debug builds keep upstream's `herdr-dev` directory, and
`build.rs` aliases `CARGO_BIN_EXE_herdr` to the `neoherdr` binary, so
upstream's tests stay byte-identical.

`AGENTS.md` (imported at the end) is upstream herdr's guide and is kept
byte-identical to upstream so rebases stay clean. It calls the project
"Herdr". Where this file and `AGENTS.md` disagree, this file wins.

## Principles

Neoherdr is an opinionated fork: build what has been decided, not what a
hypothetical user might someday want.

- **Reverse software decline.** Fight bloated clutter, forced online
  dependencies, intrusive AI features, and poorly performing applications.
- **Craftsmanship and care.** Show genuine concern for users, the hardware the
  code runs on, and writing software that is actually good.
- **Capabilities over accomplishments.** Build permanent, reusable, low-level
  solutions and components rather than temporary fixes that serve only a
  single project goal.
- **Keyboard-first.** This replaces upstream's "Herdr is a mouse-first TUI"
  under *UI patterns should be reused*: every action must be reachable and
  discoverable via the prefix/which-key keymap, with mouse support retained
  as a secondary input path. New interactive actions need a bindable
  `KeybindAction` (or plugin action) and must surface in which-key/help, not
  only a context menu.
- **No users yet.** Fork-only behavior (config files, defaults, bundled
  plugins, fork-added API methods) changes directly to the end state, with no
  migration reads, deprecation diagnostics, or compatibility shims. The
  upstream-compatibility contracts in `AGENTS.md` (config format, wire
  protocol, frozen generation-1 endpoint codecs) still apply.

## Sections of AGENTS.md that do not apply here

Ignore these in this checkout:

- **Scope and Audience**, **Maintainer Workflow**, **Local Can Machine
  Workflow**, **Release Channels**, and **External contributor guardrail**.
  They govern upstream's repository, maintainers, and release pipeline. Do
  not open issues or pull requests against `herdrdev/herdr`.
- In **Docs**: the rules about `skills/herdr/SKILL.md`, `docs/preview/`,
  `docs/versions/`, `distribution/latest.json`, and `docs/next/CHANGELOG.md`
  curation. The fork has no release pipeline and does not publish docs.
- `just check` needs cargo-nextest, bun, and the Windows SDK
  (`just setup-windows-cross`). When a tool is missing, run the stages that
  can run (`cargo fmt --check`, `cargo clippy --all-targets --locked -- -D
  warnings`, `cargo nextest run --locked`, the python maintenance tests) and
  say which stages were skipped.

## Fork workflow

- Commit on `master` or a local branch and push to `danctorres/neoherdr`.
  Before committing, propose the commit message and get alignment. Use
  lowercase conventional commits, no emojis, and no AI co-author lines.
- Plan non-trivial changes in `.local/prd/` (ignored by git).
- Rebase `master` onto `upstream/master` weekly and before starting a
  non-trivial change, never in the middle of one, then force-push
  `origin/master`.
- The fork has no update source: `neoherdr update` does not download
  anything, and SSH remotes receive the local binary. Build and install with
  `cargo install --path . --locked`, which installs the `neoherdr` command.
- Keep upstream-owned files (`AGENTS.md`, `.zed/`, CI workflows, release
  scripts) unchanged unless the fork needs different behavior; every edit to
  them is a permanent rebase conflict.

@AGENTS.md
