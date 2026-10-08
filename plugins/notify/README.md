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

`neoherdr plugin config-dir notify` prints the plugin's config directory if yours differs. Without a `url` file
the plugin does nothing. Turn it off with `neoherdr plugin disable notify`.

## Check

Send a test push to the URL you put in the `url` file. If your phone buzzes, the plugin will
reach it too:

```bash
curl -d "test from neoherdr" https://ntfy.example.ts.net/my-topic
```

The plugin's own self-check needs no network:

```bash
python3 bin/notify test
```

## Self-hosted ntfy on a tailnet

This runs ntfy on the same machine as neoherdr and reaches it from the phone over Tailscale, so
nothing leaves your tailnet. It was set up this way on Linux (WSL2 with systemd) and an Android
phone.

1. Install the `ntfy` binary ([ntfy install docs](https://docs.ntfy.sh/install/)) and find the
   machine's tailnet name:

   ```bash
   tailscale status --json | grep DNSName | head -1
   ```

   It prints something like `"DNSName": "pc.tail1234.ts.net."`. The name without the trailing
   dot is your machine's address.

2. Set the port and address in `/etc/ntfy/server.yml`. The `base-url` is `http://`, that name,
   then `:2586`, for example `http://pc.tail1234.ts.net:2586`. The server speaks plain HTTP, so
   it must be `http://`, not `https://`:

   ```yaml
   listen-http: ":2586"
   base-url: "http://<machine>.<tailnet>.ts.net:2586"
   ```

3. Start it at boot as a user service. Save this as `~/.config/systemd/user/ntfy.service`:

   ```ini
   [Unit]
   Description=ntfy notification server

   [Service]
   ExecStart=/usr/local/bin/ntfy serve
   Restart=on-failure

   [Install]
   WantedBy=default.target
   ```

   ```bash
   systemctl --user enable --now ntfy
   loginctl enable-linger "$USER"   # start without waiting for a login
   ```

4. Point the plugin at it. ntfy is on the same machine, so localhost is enough:

   ```bash
   echo 'http://localhost:2586/neoherdr' > "$(neoherdr plugin config-dir notify)/url"
   ```

5. In the ntfy app, subscribe to topic `neoherdr`, tick "use another server", and enter the
   `base-url` from step 2: only the server, with `http://`, no topic and no trailing slash. The
   subscribe button stays disabled until the topic is filled in and the address is valid.
   Tailscale must be connected on the phone.

6. Test it:

   ```bash
   curl -d "test from neoherdr" http://localhost:2586/neoherdr
   ```

On Android, accept the app's offer to switch to WebSockets, and exempt ntfy from battery
optimisation if notifications arrive late. iPhone was not tried; ntfy's docs say a self-hosted
server needs `upstream-base-url: "https://ntfy.sh"` there for timely delivery.

The server keeps messages in memory, so a restart clears undelivered ones. On WSL2 the service
only runs while WSL itself is running.
