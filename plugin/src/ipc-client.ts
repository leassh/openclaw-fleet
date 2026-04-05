import { ChildProcess, spawn } from "child_process";
import { createInterface, Interface } from "readline";
import { EventEmitter } from "events";
import * as path from "path";
import * as fs from "fs";

interface JsonRpcRequest {
  jsonrpc: "2.0";
  id: number;
  method: string;
  params?: Record<string, unknown>;
}

interface JsonRpcResponse {
  jsonrpc: "2.0";
  id?: number;
  result?: unknown;
  error?: { code: number; message: string; data?: unknown };
  method?: string;
  params?: unknown;
}

interface PendingRequest {
  resolve: (value: unknown) => void;
  reject: (reason: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

const REQUEST_TIMEOUT_MS = 30_000;

/**
 * Spawns the openclaw-fleet Rust binary and communicates via JSON-RPC over stdin/stdout.
 * Logs from the binary arrive on stderr. Event notifications arrive as JSON-RPC
 * messages without an `id`.
 */
export class FleetBinary extends EventEmitter {
  private configPath: string;
  private process: ChildProcess | null = null;
  private reader: Interface | null = null;
  private nextId = 1;
  private pending: Map<number, PendingRequest> = new Map();

  constructor(configPath: string) {
    super();
    this.configPath = configPath;
  }

  /** Resolve the binary path, preferring release over debug. */
  private resolveBinaryPath(): string {
    const base = path.resolve(__dirname, "..", "..", "binary", "target");
    const release = path.join(base, "release", "openclaw-fleet");
    const debug = path.join(base, "debug", "openclaw-fleet");

    if (fs.existsSync(release)) return release;
    if (fs.existsSync(debug)) return debug;
    throw new Error(
      `openclaw-fleet binary not found. Looked in:\n  ${release}\n  ${debug}`
    );
  }

  /** Spawn the binary and wire up stdin/stdout/stderr. */
  start(): void {
    if (this.process) {
      throw new Error("Binary already running");
    }

    const binPath = this.resolveBinaryPath();

    this.process = spawn(binPath, [this.configPath], {
      stdio: ["pipe", "pipe", "pipe"],
    });

    // Read JSON-RPC responses/events line-by-line from stdout
    this.reader = createInterface({ input: this.process.stdout! });
    this.reader.on("line", (line: string) => this.handleLine(line));

    // Forward stderr as log events
    this.process.stderr!.on("data", (chunk: Buffer) => {
      const text = chunk.toString().trimEnd();
      if (text) this.emit("log", text);
    });

    this.process.on("exit", (code, signal) => {
      this.cleanup();
      this.emit("exit", code, signal);
    });

    this.process.on("error", (err) => {
      this.cleanup();
      this.emit("error", err);
    });
  }

  /** Send a JSON-RPC request and wait for the matching response. */
  request(method: string, params?: Record<string, unknown>): Promise<unknown> {
    if (!this.process || !this.process.stdin) {
      return Promise.reject(new Error("Binary not running"));
    }

    const id = this.nextId++;
    const msg: JsonRpcRequest = { jsonrpc: "2.0", id, method };
    if (params) msg.params = params;

    return new Promise<unknown>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`Request ${method} (id=${id}) timed out after ${REQUEST_TIMEOUT_MS}ms`));
      }, REQUEST_TIMEOUT_MS);

      this.pending.set(id, { resolve, reject, timer });
      this.process!.stdin!.write(JSON.stringify(msg) + "\n");
    });
  }

  /** Get the full fleet status. */
  async getFleetStatus(): Promise<unknown> {
    return this.request("get_fleet_status");
  }

  /** Get detailed information for a single node. */
  async getNodeDetail(node: string): Promise<unknown> {
    return this.request("get_node_detail", { node });
  }

  /** Get trend data for a single node. */
  async getNodeTrend(node: string): Promise<unknown> {
    return this.request("get_trend", { node });
  }

  /** Get the value gap tracker data. */
  async getValueGap(): Promise<unknown> {
    return this.request("get_value_gap");
  }

  /** Stop the binary process. */
  stop(): void {
    if (this.process) {
      this.process.kill("SIGTERM");
      // Give it a moment, then force-kill
      setTimeout(() => {
        if (this.process) {
          this.process.kill("SIGKILL");
        }
      }, 3000);
    }
  }

  private handleLine(line: string): void {
    let msg: JsonRpcResponse;
    try {
      msg = JSON.parse(line);
    } catch {
      this.emit("log", `[ipc] unparseable line: ${line}`);
      return;
    }

    // If it has an id, it is a response to a pending request
    if (msg.id != null) {
      const pending = this.pending.get(msg.id);
      if (pending) {
        this.pending.delete(msg.id);
        clearTimeout(pending.timer);
        if (msg.error) {
          pending.reject(
            new Error(`RPC error ${msg.error.code}: ${msg.error.message}`)
          );
        } else {
          pending.resolve(msg.result);
        }
      }
      return;
    }

    // Otherwise it is an event notification (no id)
    if (msg.method) {
      this.emit("event", msg.method, msg.params);
    }
  }

  private cleanup(): void {
    // Reject all pending requests
    for (const [id, pending] of this.pending) {
      clearTimeout(pending.timer);
      pending.reject(new Error("Binary exited while request was pending"));
      this.pending.delete(id);
    }
    this.reader?.close();
    this.reader = null;
    this.process = null;
  }
}
