# subagents

Opens a pane for each subagent an agent starts, so you can watch it work.

Agents run their subagents inside their own process, so neoherdr has no terminal to show for
them. This plugin follows the record each agent keeps of its subagents instead.

| Agent | Where subagents are read from |
|---|---|
| Claude Code | `<config>/projects/*/<session>/subagents/*.jsonl` |
| omp | the directory named after the parent session file |
| pi | same as omp, for a subagent extension that saves sessions the same way |
| OpenCode | child sessions in `opencode.db` |

## What it does

- When an agent starts working, a watcher polls its session once a second for new subagents.
- Each new subagent gets a pane labelled with its name. The first opens beside the agent and
  the rest stack below it, sharing the height evenly. Focus stays where it was.
- The pane shows the task, one line per tool call, and what the subagent says.
- The pane closes 20 seconds after the subagent finishes, or after 10 minutes without output.

It needs `python3` on your `PATH` and runs on Linux and macOS.

## Install

```bash
neoherdr plugin link /path/to/neoherdr/plugins/subagents
```

Turn it off with `neoherdr plugin disable subagents`.

## Check

```bash
python3 bin/subagents test
```
