# Glass Deployment Contract

Glass runs as one Rust process with one SQLite database. The verified-live
deployment proof is always:

```sh
glass doctor --url <served-url> --db <expected-db>
```

The doctor must succeed before the service is called live.

## Local Service

Build the binary from the checkout that will be supervised:

```sh
cd $HOME/Development/glass
cargo build --release
mkdir -p .glass-live
```

The local service command is:

```sh
$HOME/Development/glass/target/release/glass serve \
  --bind 127.0.0.1:9040 \
  --db $HOME/Development/glass/.glass-live/glass.db
```

Set `GLASS_SANCTUM_URL` to the portal root if this deployment sits behind a
Sanctum portal (see the viewer's cross-repo home affordance, glass-915); left
unset, the affordance falls back to an inert same-origin link.

Set `GLASS_FLEET_RETRO_SHELF_URL` to the bastion artifact shelf's fleet-retro
publish base (e.g. `https://<your-tailnet-host>/artifacts/a/fleet-retro`) to
enable REP-1's window report (`GET /api/window-report/{daily,weekly}`,
glass-917). Left unset, that route returns a clear "not configured" error
event rather than a hardcoded personal shelf.

Starting fresh on posts is acceptable for the campaign cutover when migration is
not explicitly required. If preserving an existing native Glass stage, reuse the
same `.glass-live/glass.db` file. Do not point Glass at the retired Sideshow DB;
the schemas are different.

## Launchd

The workstation supervision surface is a user LaunchAgent. The label for native
Glass is `com.<user>.glass` (substitute your own reverse-DNS namespace).

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>com.&lt;user&gt;.glass</string>
  <key>ProgramArguments</key><array>
    <string>/bin/zsh</string><string>-lc</string>
    <string>cd $HOME/Development/glass &amp;&amp; exec $HOME/Development/glass/target/release/glass serve --bind 127.0.0.1:9040 --db $HOME/Development/glass/.glass-live/glass.db</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>/tmp/glass.log</string>
  <key>StandardErrorPath</key><string>/tmp/glass.log</string>
</dict></plist>
```

Install and start:

```sh
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.<user>.glass.plist
launchctl kickstart -k gui/$(id -u)/com.<user>.glass
launchctl list | rg 'com.<user>.glass'
```

## Tailnet Slot

The campaign slot is a tailnet HTTPS hostname of your choosing, e.g.:

```text
https://<your-tailnet-host>:9040
```

It should proxy to:

```text
http://127.0.0.1:9040
```

Configure or confirm with:

```sh
tailscale serve --bg --https 9040 http://127.0.0.1:9040
tailscale serve status --json
```

The status JSON must show `<your-tailnet-host>:9040` with `/` proxied to
`http://127.0.0.1:9040`.

## Cutover From Interim Sideshow

The interim deployment used `com.<user>.sideshow` with `npx sideshow serve
--port 9040`. A Glass cutover is allowed for campaign lanes only when all of the
following are true:

- the lane has claimed the Powder card that explicitly asks for verified-live
  Glass deployment;
- the repo gate is green on the branch being deployed;
- the PR has merged to the default branch, or the operator has explicitly
  requested a pre-merge local deployment;
- `glass doctor` succeeds locally against the exact DB path that launchd will
  use;
- rollback keeps the old Sideshow plist available.

Cutover sequence:

```sh
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.<user>.sideshow.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.<user>.glass.plist
launchctl kickstart -k gui/$(id -u)/com.<user>.glass
tailscale serve --bg --https 9040 http://127.0.0.1:9040
glass doctor --url http://127.0.0.1:9040 --db $HOME/Development/glass/.glass-live/glass.db
curl -sS -I https://<your-tailnet-host>:9040/
```

## Rollback

Rollback restores the old launchd owner of port `9040`:

```sh
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.<user>.glass.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.<user>.sideshow.plist
launchctl kickstart -k gui/$(id -u)/com.<user>.sideshow
tailscale serve --bg --https 9040 http://127.0.0.1:9040
```

After rollback, `curl -sS http://127.0.0.1:9040/` should show the interim
Sideshow viewer again.
