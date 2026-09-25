# Server Deployment and Hosting Guide

This directory provides production-tested systemd service units and deployment configurations for hosting BlueEngine games and authoritative dedicated servers on Linux hosts.

## Files

- `systemd/blueengine-server.service.example`: Hardened systemd unit file with resource limits, isolation, and auto-restart.

## Hardening Defaults

The example service includes:
- `NoNewPrivileges=true`: Disallows privilege escalation.
- `PrivateTmp=true`: Isolates `/tmp` directory.
- `UMask=0077`: Strict file permissions for files created by the process.
- `MemoryMax=512M`: Out-of-memory circuit breaker preventing system-wide exhaustion.
- `TasksMax=32`: Prevents fork bomb / thread explosion DOS.
- `EnvironmentFile`: Keeps join keys and TLS private keys out of command lines and process listings.

## Quick Installation on Linux Host

1. Copy the executable to your user binary path:
   ```bash
   mkdir -p ~/.local/bin ~/.config/blueengine
   cp target/release/be2-headless ~/.local/bin/
   ```

2. Create private environment file:
   ```bash
   cat << 'EOF' > ~/.config/blueengine/server.env
   BLUE_JOIN_KEY=replace-with-your-secure-join-key
   BLUE_TLS_KEY_FILE=/home/user/.config/blueengine/server-key.der
   EOF
   chmod 600 ~/.config/blueengine/server.env
   ```

3. Install user systemd service:
   ```bash
   mkdir -p ~/.config/systemd/user
   cp deploy/systemd/blueengine-server.service.example ~/.config/systemd/user/blueengine-server.service
   systemctl --user daemon-reload
   systemctl --user enable --now blueengine-server.service
   ```

4. Check logs:
   ```bash
   journalctl --user -u blueengine-server.service -f
   ```
