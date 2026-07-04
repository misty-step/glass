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
cd /Users/phaedrus/Development/glass
cargo build --release
mkdir -p .glass-live
```

The local service command is:

```sh
/Users/phaedrus/Development/glass/target/release/glass serve \
  --bind 127.0.0.1:9040 \
  --db /Users/phaedrus/Development/glass/.glass-live/glass.db
```

Starting fresh on posts is acceptable for the campaign cutover when migration is
not explicitly required. If preserving an existing native Glass stage, reuse the
same `.glass-live/glass.db` file. Do not point Glass at the retired Sideshow DB;
the schemas are different.

## Launchd

The workstation supervision surface is a user LaunchAgent. The label for native
Glass is `com.phaedrus.glass`.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>com.phaedrus.glass</string>
  <key>ProgramArguments</key><array>
    <string>/bin/zsh</string><string>-lc</string>
    <string>cd /Users/phaedrus/Development/glass &amp;&amp; exec /Users/phaedrus/Development/glass/target/release/glass serve --bind 127.0.0.1:9040 --db /Users/phaedrus/Development/glass/.glass-live/glass.db</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>/tmp/glass.log</string>
  <key>StandardErrorPath</key><string>/tmp/glass.log</string>
</dict></plist>
```

Install and start:

```sh
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.phaedrus.glass.plist
launchctl kickstart -k gui/$(id -u)/com.phaedrus.glass
launchctl list | rg 'com.phaedrus.glass'
```

## Tailnet Slot

The campaign slot is:

```text
https://serenity.tail5f5eb4.ts.net:9040
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

The status JSON must show `serenity.tail5f5eb4.ts.net:9040` with `/` proxied to
`http://127.0.0.1:9040`.

## Cutover From Interim Sideshow

The interim deployment used `com.phaedrus.sideshow` with `npx sideshow serve
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
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.phaedrus.sideshow.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.phaedrus.glass.plist
launchctl kickstart -k gui/$(id -u)/com.phaedrus.glass
tailscale serve --bg --https 9040 http://127.0.0.1:9040
glass doctor --url http://127.0.0.1:9040 --db /Users/phaedrus/Development/glass/.glass-live/glass.db
curl -sS -I https://serenity.tail5f5eb4.ts.net:9040/
```

## Rollback

Rollback restores the old launchd owner of port `9040`:

```sh
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.phaedrus.glass.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.phaedrus.sideshow.plist
launchctl kickstart -k gui/$(id -u)/com.phaedrus.sideshow
tailscale serve --bg --https 9040 http://127.0.0.1:9040
```

After rollback, `curl -sS http://127.0.0.1:9040/` should show the interim
Sideshow viewer again.
