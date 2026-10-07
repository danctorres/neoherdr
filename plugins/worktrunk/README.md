# Worktrunk plugin for Herdr

Keybindable, which-key-visible actions that drive the third-party
[`wt` (worktrunk)](https://github.com/max-sixty/worktrunk) CLI for
git-worktree switch/create, list, remove, and merge workflows.

List, remove, and merge only invoke the external `wt` binary. Switch also
opens and focuses the chosen checkout as a Herdr space.

## Requirements

- `wt` on your `$PATH`. Without it, each action says so and offers to
  install it with Homebrew or cargo, whichever is present
- Linux or macOS
- Herdr 0.9.0 or newer

## Install

Herdr bundles this plugin and registers it on start. `herdr plugin disable
worktrunk` hides its menu entries; `herdr plugin unlink worktrunk` removes it
for good. To bring it back, link it again:

```bash
neoherdr plugin link ~/.config/neoherdr/plugins/builtin/worktrunk
```

## Keybindings

On Linux and macOS the actions appear in Herdr's git menu by default and in
the `prefix+?` help panel under their description text. Press `prefix+g` to
open the git menu, then:

| Key | Action |
| --- | --- |
| `s` | switch or create a worktree |
| `l` | list worktrees |
| `m` | merge the current worktree |

After a successful switch, Herdr opens that checkout as a space and focuses
it. `esc` returns to the top-level prefix menu.

`remove` has no default key. Add it, or move any action, with a
`[[keys.command]]` entry that joins the git menu with a bare key. A user entry
on `s`, `l`, or `m` replaces the default worktrunk entry on that key:

```toml
[[keys.command]]
key = "d"
group = "git"
type = "plugin_action"
command = "worktrunk.remove"
description = "worktrunk: remove"
```

The `command` ids are the only fixed part. Verify with
`herdr plugin action list --plugin worktrunk`.

To create a worktree from the switch picker, type the new branch name
first and then press `alt+c` — pressing `alt+c` with an empty picker makes
`wt` exit with "no branch name entered".

## New worktrees appear automatically

`wt`'s `post-start` hook runs once per created worktree (unlike
`post-switch`, which fires on every switch). Add this to your
`~/.config/worktrunk/config.toml` so each new worktree opens as a Herdr
workspace:

```toml
post-start = "${HERDR_BIN_PATH:-herdr} worktree open --cwd '{{ repo_path }}' --path '{{ worktree_path }}' --focus"
```

Use `${HERDR_BIN_PATH:-herdr}`, not a bare `herdr`: inside a Herdr pane it
resolves to the running server's binary, while a bare `herdr` can resolve
to an older installed release whose protocol the server rejects (the hook
then fails silently in the background).

## How it works

The action is a thin wrapper that reads the focused pane's working directory
from `HERDR_PLUGIN_CONTEXT_JSON` and opens the `wt switch` entrypoint as a
popup in that directory. All prompts and pickers are worktrunk's own;
this plugin parses no output and collects no input first. After a successful
pick it runs `herdr worktree open --focus` so the checkout appears in Spaces
and takes focus. If `wt` exits with an error, the pane stays open showing
the error and waits for Enter instead of closing immediately.
