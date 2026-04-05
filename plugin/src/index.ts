// OpenClaw plugin API types -- using 'any' until SDK package is available

import * as path from "path";
import * as fs from "fs";
import { FleetBinary } from "./ipc-client";
import { registerTools } from "./tools";

const DASHBOARD_PATH = path.resolve(__dirname, "..", "assets", "dashboard.html");

export default {
  id: "openclaw-fleet",
  name: "Fleet Monitor",

  register(api: any): void {
    // Resolve fleet config path
    const configPath: string =
      api.pluginConfig?.configPath ||
      path.join(process.cwd(), "fleet.yaml");

    // Create the IPC binary wrapper
    const binary = new FleetBinary(configPath);

    // Register tools (fleet_status, node_detail, get_value_gap)
    registerTools(api, binary);

    // --- HTTP routes ---

    // Serve the dashboard HTML
    api.registerRoute("GET", "/fleet", (_req: any, res: any) => {
      try {
        const html = fs.readFileSync(DASHBOARD_PATH, "utf-8");
        res.setHeader("Content-Type", "text/html; charset=utf-8");
        res.end(html);
      } catch (err: any) {
        res.statusCode = 500;
        res.end(`Dashboard not found: ${err.message}`);
      }
    });

    // JSON API endpoint for the dashboard
    api.registerRoute("GET", "/fleet/api/status", async (_req: any, res: any) => {
      try {
        const status = await binary.getFleetStatus();
        res.setHeader("Content-Type", "application/json");
        res.end(JSON.stringify(status));
      } catch (err: any) {
        res.statusCode = 502;
        res.setHeader("Content-Type", "application/json");
        res.end(JSON.stringify({ error: err.message }));
      }
    });

    // --- Background service: start/stop the binary with the plugin lifecycle ---
    api.registerService({
      name: "openclaw-fleet-binary",
      start: () => {
        binary.start();
        binary.on("log", (msg: string) => {
          api.log?.("[openclaw-fleet]", msg);
        });
        binary.on("event", (method: string, params: unknown) => {
          api.log?.("[openclaw-fleet event]", method, params);
        });
        binary.on("exit", (code: number | null, signal: string | null) => {
          api.log?.(`[openclaw-fleet] binary exited code=${code} signal=${signal}`);
        });
      },
      stop: () => {
        binary.stop();
      },
    });
  },
};
