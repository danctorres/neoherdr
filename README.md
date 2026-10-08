# neoherdr

> **neoherdr is a fork of [herdr](https://github.com/herdrdev/herdr) with a different philosophy: keyboard-first instead of mouse-first.** Every action is reachable and discoverable through a prefix/which-key keymap; the mouse stays supported as a secondary input path, not the primary design target. The command is `neoherdr`, with its own config directory, so it installs and runs beside upstream herdr; subcommands, config format, and protocol stay compatible.

<p align="center">
  <img src="assets/logo.png" alt="herdr" width="100" />
</p>

<p align="center">
  <a href="https://herdr.dev">herdr.dev</a> · <a href="#install">install</a> · <a href="https://herdr.dev/docs/quick-start/">quick start</a> · <a href="https://herdr.dev/docs/">docs</a>
</p>

<p align="center">
  English · <a href="README.zh-CN.md">简体中文</a>
</p>


<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-666666?labelColor=333333" alt="Apache 2.0 license" /></a>
</p>

---

## what neoherdr changes

- **which-key menus** — `prefix` opens a grouped menu of every action; the menus are config, so you can rearrange them or add built-in actions to your own. [keyboard →](docs/next/website/src/content/docs/keyboard.mdx)
- **keymap rules** — default keys follow five written rules (hot path is one key, menus are nouns, one verb vocabulary, one key per action, keys mirror the screen), so you can guess most bindings.
- **agent picker** — `prefix+a n` lists the agents installed on the session's machine and opens one in a new tab.
- **configured tuis** — declare `[[tui]]` entries in `config.toml` and open them from `prefix+o`. [configuration →](docs/next/website/src/content/docs/configuration.mdx)
- **failed panes stay open** — an agent or tui that exits with an error keeps its output on screen until you press enter.
- **launching through your shell** — agents and tuis start through your interactive shell, so rc-file `PATH` and exported keys reach them.
- **bundled worktrunk** — `prefix+g` then `s`, `l`, or `m` drives the [`wt`](https://worktrunk.dev) worktree cli; without `wt`, the popup offers to install it.
- **touch reaches the keymap** — on a phone-width terminal the header gains a `keys` button that opens the which-key menu, and every menu row is a tap target (clicks work on the desktop too).
- **phone notifications** — the [`notify` plugin](plugins/notify/README.md) pushes to a url you choose (ntfy, for example) when an agent needs your input or finishes somewhere you are not looking.
- **runs beside upstream** — the command is `neoherdr` and its config, sessions, and sockets live in `~/.config/neoherdr`.

everything else is upstream herdr.

---

**the runtime your coding agents live on.**

- **detach without stopping work** — herdr keeps terminals running in a background server when you close the client or lose your SSH connection. after a server or machine restart, herdr restores the saved layout and can resume supported agent sessions; the original processes do not survive. [session state →](https://herdr.dev/docs/session-state/)
- **several machines, one window** — keep local work and saved ssh machines together, with a combined agent list and independent reconnects. [remote machines →](https://herdr.dev/docs/connecting-machines/)
- **never hunt for the stuck one** — every pane is marked working, blocked, or idle. when an agent stops and needs an answer, herdr says so.
- **agent-native** — agents drive herdr through the cli and socket api: they can spawn panes, prompt each other, and wait until another agent is genuinely blocked. [agent skill →](https://herdr.dev/docs/agent-skill/)
- **runs what you already run** — claude code, codex, cursor, opencode, grok and the rest. herdr doesn't wrap or replace them; it owns their terminals. building an agent? [add herdr support →](https://herdr.dev/docs/add-herdr-support/)
- **keyboard-first** — a discoverable prefix/which-key keymap gets you everywhere without memorizing it; mouse click, drag, and split stay fully supported as a secondary path.
- **plugins** — extend panes and workflows. [browse the marketplace →](https://herdr.dev/plugins/)
- **one rust binary, no electron** — runs in whatever terminal you already use.

---

## install

neoherdr has no prebuilt binaries or package-manager releases; the herdr.dev installer, brew, and mise install upstream herdr instead. build it from source (needs rust and zig 0.16.0, see [development](#development)):

```bash
cargo install --locked --git https://github.com/danctorres/neoherdr
```

this installs the `neoherdr` command, which keeps its config, sessions, and sockets in `~/.config/neoherdr`. upstream herdr can stay installed beside it: the two share nothing. everything else is spelled as upstream spells it (subcommands, `HERDR_*` variables, config keys), so upstream's docs apply with `neoherdr` in place of `herdr`. `neoherdr update` does not download anything in this fork; rerun the command above to update. ssh remotes receive the local binary as `~/.local/bin/neoherdr`. inside a neoherdr pane, `herdr` still works: it resolves to upstream herdr when that is installed, otherwise to neoherdr itself.

then start it where the work lives:

```bash
neoherdr
```

run your agents, split panes, walk away. `ctrl+b q` detaches, `neoherdr` reattaches. [quick start →](https://herdr.dev/docs/quick-start/)

## docs

the keymap and configuration differ from upstream, so read this fork's pages in the repo: [keyboard](docs/next/website/src/content/docs/keyboard.mdx) · [configuration](docs/next/website/src/content/docs/configuration.mdx). inside neoherdr, `prefix` opens the which-key menu and `prefix ?` shows every binding.

everything else is shared with upstream at [herdr.dev/docs](https://herdr.dev/docs/): [quick start](https://herdr.dev/docs/quick-start/) · [concepts](https://herdr.dev/docs/concepts/) · [supported agents](https://herdr.dev/docs/agents/) · [session state](https://herdr.dev/docs/session-state/) · [connecting machines](https://herdr.dev/docs/connecting-machines/) · [remote](https://herdr.dev/docs/persistence-remote/) · [integrations](https://herdr.dev/docs/integrations/) · [plugins](https://herdr.dev/docs/plugins/) · [socket api](https://herdr.dev/docs/socket-api/)

## agent instructions

if you are an ai agent helping with this repository, read [`CLAUDE.md`](./CLAUDE.md) (fork rules) and [`AGENTS.md`](./AGENTS.md) (upstream rules) before making changes.

## development

prerequisites: rust 1.96.1 (pinned in `rust-toolchain.toml`, installed automatically by rustup) and zig 0.16.0 on your `PATH` (or pointed to by the `ZIG` env var) — the build shells out to `zig build` for the vendored terminal library, and fails without it. [install zig →](https://ziglang.org/download/)

```bash
git clone https://github.com/danctorres/neoherdr
cd neoherdr
cargo build --release   # or plain `cargo build` for a faster debug build
```

run what you built:

```bash
./target/release/neoherdr
```

if you are already inside a herdr session, clear the inherited socket overrides so the new binary talks to its own dev server instead of the running one:

```bash
env -u HERDR_SOCKET_PATH -u HERDR_CLIENT_SOCKET_PATH cargo run -- <command>
```

`just` recipes wrap the common workflows — install [just](https://just.systems/) first, then:

```bash
just test        # unit tests
just check       # formatting, tests, and maintenance checks
```

## license

Herdr is licensed under the [Apache License 2.0](LICENSE).
