# openclaw-fleet

> Talk to your infrastructure. SSH-based fleet monitoring for OpenClaw.

Ask your AI agent about your machines -- and get real-time answers.

## What it does

- **Monitor** any machine via SSH -- CPU, GPU, RAM, disk, processes, idle time, logged-in users
- **Ask** your agent: "how's the fleet?", "what's .179 doing?", "is Ollama running?"
- **See** a live fleet dashboard at `/fleet` with real-time metrics
- **Get alerts** when things go wrong -- node offline, disk critical, temperature warnings
- **Detect trends** -- "disk dropping at 12GB/hour, critical in ~40 minutes"

Zero software installed on monitored machines. If SSH works, this works.

## Quick Start

```bash
openclaw plugins install openclaw-fleet
```

Then ask your agent: "How's the fleet?"

## Setup

Add machines to `fleet.yaml`:

```yaml
nodes:
  - name: gpu-server
    host: 192.168.1.131
    ssh: user@192.168.1.131
    os: windows

  - name: macbook
    host: 192.168.1.129
    ssh: user@192.168.1.129
    os: macos
```

Dashboard: `http://localhost:18789/fleet`

## Supported Platforms

| OS | CPU | RAM | Disk | GPU | Idle | Users | Processes |
|----|-----|-----|------|-----|------|-------|-----------|
| Linux | Yes | Yes | Yes | Yes (nvidia-smi) | Yes | Yes | Yes |
| macOS | Yes | Yes | Yes | - | Yes | Yes | Yes |
| Windows | Yes | Yes | Yes | Yes (nvidia-smi) | Yes | Yes | Yes |

## Want More?

openclaw-fleet monitors and alerts. **[Leassh](https://leassh.com)** acts.

| | openclaw-fleet (free) | Leassh Pro |
|---|---|---|
| SSH monitoring | Unlimited | Yes |
| Chat with your agent | Yes | Yes |
| Fleet dashboard | Yes | Yes |
| Proactive alerts | Yes | Yes |
| Execute commands | No | Yes |
| Kill processes | No | Yes |
| Rules & automation | No | Yes |
| Webhooks & MQTT | No | Yes |
| Screenshot analysis | No | Yes |
| Screen time tracking | No | Yes (Family) |
| AI content safety | No | Yes (Family) |
| Time limits & enforcement | No | Yes (Family) |
| Telegram notifications | No | Yes |

[Get Leassh](https://leassh.com)

## License

MIT
