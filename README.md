# openclaw-fleet

> Talk to your infrastructure. SSH-based fleet monitoring for OpenClaw.

Ask your AI agent about your machines -- and get real-time answers.

```
You:    How's the fleet?

Agent:  All 6 nodes online.

        gpu-primary    Active   CPU 12%  GPU 87%  RAM 74%  Disk 142GB free
        gpu-secondary  Active   CPU  3%  GPU 91%  RAM 68%  Disk 89GB free
        build-server   Active   CPU 45%  GPU --   RAM 82%  Disk 34GB free
        macbook        Away     CPU  8%  GPU --   RAM 61%  Disk 203GB free
        ronaldo-pc     Idle     CPU  2%  GPU 94%  RAM 45%  Disk 512GB free
        nas            Active   CPU  1%  GPU --   RAM 23%  Disk 2.1TB free

        ronaldo-pc has FortniteClient.exe using 94% GPU.
        Nobody's touched the keyboard in 47 minutes.
```

Zero software installed on monitored machines. If SSH works, this works.

---

## What It Does

openclaw-fleet is an OpenClaw plugin that connects to your machines over SSH and collects system metrics: CPU, GPU, RAM, disk, running processes, idle time, logged-in users, and temperature. Your AI agent can then answer questions about your infrastructure in natural language, alert you proactively when something goes wrong, and detect trends before they become problems.

The Rust binary handles SSH connections and metric collection. The TypeScript plugin exposes tools to the OpenClaw agent and serves a live dashboard. Communication between the two happens over JSON-RPC on stdin/stdout.

## Features

- **Real-time monitoring** -- CPU, GPU (nvidia-smi), VRAM, RAM, disk, temperature, processes, idle time, logged-in users
- **Natural language** -- "What's .179 doing?" / "Is Ollama running?" / "Who's logged into .131?"
- **Proactive alerts** -- Node offline, disk critically low, disk filling fast, RAM exhaustion, temperature critical, SSH auth failures, login/logout events
- **Trend detection** -- Linear regression over rolling metric windows; predicts when disk will fill, temperatures will overheat
- **Live dashboard** -- Dark-themed single-page fleet view at `/fleet` with auto-refresh, leash tension bars, expandable process lists
- **Cross-platform** -- Linux, macOS, Windows targets from a single binary
- **Zero agent install** -- Nothing runs on monitored machines; SSH key auth is all you need
- **Configurable thresholds** -- Probe intervals, load classifications, idle timeouts, all in YAML

## Quick Start

```bash
openclaw plugins install openclaw-fleet
```

Create `fleet.yaml` in your OpenClaw workspace:

```yaml
nodes:
  - name: gpu-server
    host: 192.168.1.131
    ssh: user@192.168.1.131
    os: linux
    gpu: nvidia

  - name: macbook
    host: 192.168.1.129
    ssh: user@192.168.1.129
    os: macos

  - name: windows-desktop
    host: 192.168.1.122
    ssh: user@192.168.1.122
    os: windows
    gpu: nvidia

probes:
  health_interval: 60     # seconds between health checks
  metrics_interval: 120   # seconds between full metric probes
  idle_threshold: 1800    # seconds before a machine is "idle"

load_thresholds:
  low: 30
  high: 70
```

Then ask your agent:

```
How's the fleet?
```

Dashboard: `http://localhost:18789/fleet`

## Chat Examples

**Fleet overview:**
```
You:    How's the fleet?
Agent:  5 of 6 nodes online. build-server went offline 3 minutes ago.
        gpu-primary is running hot -- GPU at 88C.
```

**Single node deep-dive:**
```
You:    What's gpu-primary doing?
Agent:  gpu-primary (192.168.1.131, Linux)
        CPU 14%  GPU 91%  RAM 18.4/24.0 GB (77%)  Disk 142GB free
        GPU temp 72C  VRAM 18.2/24.0 GB
        Users: carl
        Top processes:
          python train.py     -- GPU 91%, 14.2 GB RAM
          ollama serve        -- CPU 3%, 2.1 GB RAM
          node server.js      -- CPU 1%, 0.4 GB RAM
```

**Proactive alert -- your agent tells you before you ask:**
```
Agent:  Hey -- .138 just went offline. Last seen 2 minutes ago, CPU was at 3%,
        nothing unusual before that. I'll let you know when it comes back.
```

**Trend detection:**
```
Agent:  .131 disk is at 8GB free and dropping about 12GB/hour. At this rate
        it'll be critical in about 40 minutes. You might want to check
        what's filling it up.
```

## Dashboard

The fleet dashboard serves at `/fleet` -- a dark-themed, single-page view that auto-refreshes every 5 seconds.

```
 openclaw-fleet                                              ● 14:32:07
 ──────────────────────────────────────────────────────────────────────
  6 nodes    5 online    2 busy
 ──────────────────────────────────────────────────────────────────────

  ▌ gpu-primary      ACTIVE     ████████████████████░░░░  87%
  ▌   GPU 87%   CPU 12%   RAM 74%   Disk 142GB
  ▌   idle 2m / users: carl

  ▌ gpu-secondary    ACTIVE     ██████████████████████░░  91%
  ▌   GPU 91%   CPU 3%    RAM 68%   Disk 89GB

  ▌ build-server     ACTIVE     ██████████░░░░░░░░░░░░░░  45%
  ▌   GPU --    CPU 45%   RAM 82%   Disk 34GB

  ▌ macbook          AWAY       ██░░░░░░░░░░░░░░░░░░░░░░   8%
  ▌   GPU --    CPU 8%    RAM 61%   Disk 203GB

  ▌ ronaldo-pc       IDLE       ███████████████████████░  94%
  ▌   GPU 94%   CPU 2%    RAM 45%   Disk 512GB
  ▌   idle 47m / users: ronaldo / FortniteClient.exe

  ▌ nas              ACTIVE     ░░░░░░░░░░░░░░░░░░░░░░░░   1%
  ▌   GPU --    CPU 1%    RAM 23%   Disk 2.1TB

 ──────────────────────────────────────────────────────────────────────
  Recent Events
  14:30  ● gpu-primary load: High
  14:28  ● build-server Active → Away → Active
  14:15  ○ nas user login: carl → 1 users
```

Each node row has a "leash tension" bar -- a visual fill that represents the max of CPU and GPU load. Colors shift from dim (idle) through amber (medium) to red (high). Click a node to expand and see its full process table.

## Built-in Triggers

These fire automatically and your agent reports them in natural language:

| Trigger | Fires when | Severity |
|---|---|---|
| `node_offline` | Node unreachable for 2+ consecutive probes | Critical |
| `node_back_online` | Previously offline node responds again | Info |
| `disk_critically_low` | Free disk < 2 GB | Critical |
| `disk_filling_fast` | Predicted < 2 GB within 1 hour | Critical |
| `disk_steady_drain` | Depletion rate > 10 GB/hour | Warning |
| `ram_exhaustion` | RAM usage > 95% | Critical |
| `temperature_critical` | GPU > 90C or CPU > 95C | Warning |
| `ssh_auth_failure` | First SSH probe failure for a node | Warning |
| `login_event` | User logs in or out | Info |

## Supported Platforms

Monitored machines (targets):

| OS | CPU | RAM | Disk | GPU | VRAM | Temp | Idle | Users | Processes |
|----|-----|-----|------|-----|------|------|------|-------|-----------|
| Linux | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Yes |
| macOS | Yes | Yes | Yes | -- | -- | -- | Yes | Yes | Yes |
| Windows | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Yes |

GPU monitoring requires `nvidia-smi` in PATH on the target machine.

The binary itself runs on Linux. Cross-compilation for macOS and Windows is straightforward with `cross`.

## How It Works

```
                        ┌──────────────────────┐
                        │   OpenClaw Agent      │
                        │   (your AI)           │
                        └────────┬─────────────┘
                                 │ tools: fleet_status
                                 │         node_detail
                                 │         get_value_gap
                        ┌────────┴─────────────┐
                        │   TypeScript Plugin   │
                        │   (registers tools,   │
                        │    serves dashboard)  │
                        └────────┬─────────────┘
                                 │ JSON-RPC over stdin/stdout
                        ┌────────┴─────────────┐
                        │   Rust Binary         │
                        │   (SSH probes,        │
                        │    state machine,     │
                        │    trend tracker,     │
                        │    trigger engine)    │
                        └────────┬─────────────┘
                                 │ SSH (key auth)
              ┌──────────┬───────┴────────┬──────────┐
              │          │                │          │
          ┌───┴───┐  ┌───┴───┐      ┌────┴────┐  ┌──┴───┐
          │ Linux │  │ macOS │      │ Windows │  │ ...  │
          └───────┘  └───────┘      └─────────┘  └──────┘
```

The Rust binary runs a probe loop on a configurable interval. Each cycle, it SSH-es into every node in parallel, runs OS-specific commands (`/proc/stat`, `nvidia-smi`, `ps aux`, `who`, etc.), parses the output, and updates an in-memory state machine. A trend tracker fits linear regressions over rolling windows to predict when metrics will cross thresholds. Built-in triggers compare current state against previous state and fire events when conditions change.

The TypeScript plugin manages the binary's lifecycle and registers three tools with OpenClaw:

- `fleet_status` -- Returns all nodes with current metrics, activity state, load classification, and trend data
- `node_detail` -- Returns detailed info for a single node including full process list
- `get_value_gap` -- Returns the value gap tracker (situations where automated action could have helped)

The plugin also serves the dashboard HTML at `/fleet` and a JSON API at `/fleet/api/status`.

## Want More?

openclaw-fleet monitors and alerts. [Leassh](https://leassh.com) acts.

|  | openclaw-fleet (free) | Leassh Pro | Leassh Family |
|---|---|---|---|
| SSH monitoring | Unlimited | Yes | Yes |
| Natural language chat | Yes | Yes | Yes |
| Fleet dashboard | Yes | Yes | Yes |
| Proactive alerts | Yes | Yes | Yes |
| Trend prediction | Yes | Yes | Yes |
| Execute commands | -- | Yes | Yes |
| Kill processes | -- | Yes | Yes |
| Rules & automation | -- | Yes | Yes |
| Webhooks & MQTT | -- | Yes | Yes |
| Screenshot analysis | -- | Yes | Yes |
| Telegram notifications | -- | Yes | Yes |
| Screen time tracking | -- | -- | Yes |
| AI content safety | -- | -- | Yes |
| Time limits & enforcement | -- | -- | Yes |

The free plugin keeps a log of situations where it could have acted but couldn't. Ask your agent "what have I missed?" for a summary:

```
Agent:  This week I spotted 14 situations where I could have helped
        with Leassh Pro:
        - 8x: idle processes hogging GPU on shared machines
        - 3x: Ollama stopped responding on .131
        - 2x: disk space warnings on gpu-primary
        - 1x: .138 went offline for 20 minutes

        That's roughly 45 minutes of manual work I could have
        handled automatically.
```

[leassh.com](https://leassh.com)

## Configuration Reference

Full `fleet.yaml` example:

```yaml
nodes:
  - name: gpu-primary        # Display name (used in chat)
    host: 192.168.1.131      # IP or hostname (for display)
    ssh: carl@192.168.1.131  # SSH target (user@host or user@host:port)
    os: linux                # linux, macos, or windows
    gpu: nvidia              # Optional: enables GPU probing
    shared: true             # Optional: marks as shared machine

  - name: windows-desktop
    host: 192.168.1.122
    ssh: carl@192.168.1.122:22
    os: windows
    gpu: nvidia

probes:
  health_interval: 60        # Seconds between connectivity checks
  metrics_interval: 120      # Seconds between full metric probes
  idle_threshold: 1800       # Seconds of no input before "idle" state

load_thresholds:
  low: 30                    # Below this % = Low load
  high: 70                   # Above this % = High load
```

### SSH Key Setup

openclaw-fleet looks for keys in `~/.ssh/` in this order: `id_ed25519`, `id_rsa`, `id_ecdsa`. Password auth is not supported -- use key-based auth.

For Windows targets, enable OpenSSH Server in Windows Settings > Apps > Optional Features, then copy your public key to `C:\Users\<user>\.ssh\authorized_keys`.

## Contributing

Contributions welcome. The codebase is small -- the Rust binary is ~1500 lines, the TypeScript plugin is ~200.

```bash
# Build the binary
cd binary && cargo build --release

# Build the plugin
cd plugin && npm install && npm run build
```

File issues on GitHub. PRs should include a clear description of what changed and why.

## License

MIT -- see [LICENSE](LICENSE).
