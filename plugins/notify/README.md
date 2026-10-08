# notify

Pushes a notification when an agent blocks or finishes, so a locked phone hears about it.

Each time an agent's status becomes `blocked` or `done`, the plugin POSTs to a URL you choose.
`done` means the agent finished somewhere you were not looking; an agent that finishes in the tab
an attached client is showing goes straight to `idle` and sends nothing.
The title is `<agent> <status>` and the body is the agent's working directory. That is the shape
[ntfy](https://ntfy.sh) expects, and a self-hosted ntfy on your tailnet keeps it off third-party
servers.

It needs `python3` on your `PATH` and runs on Linux and macOS.

## Install

```bash
neoherdr plugin link /path/to/neoherdr/plugins/notify
echo 'https://ntfy.example.ts.net/my-topic' > ~/.config/neoherdr/plugins/config/notify/url
```

`neoherdr plugin list` shows the plugin's config directory if yours differs. Without a `url` file
the plugin does nothing. Turn it off with `neoherdr plugin disable notify`.

## Check

```bash
python3 bin/notify test
```
