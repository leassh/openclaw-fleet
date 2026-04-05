// OpenClaw plugin API types -- using 'any' until SDK package is available

import { FleetBinary } from "./ipc-client";

/**
 * Register openclaw-fleet tools with the OpenClaw plugin API.
 * Read-only: fleet_status, node_detail, get_value_gap.
 */
export function registerTools(api: any, binary: FleetBinary): void {
  api.registerTool({
    name: "fleet_status",
    description:
      "Get the current status of all nodes in the fleet -- connectivity, load, GPU, logged-in users.",
    parameters: {},
    execute: async () => {
      const result = await binary.getFleetStatus();
      return JSON.stringify(result, null, 2);
    },
  });

  api.registerTool({
    name: "node_detail",
    description:
      "Get detailed information about a specific fleet node including running processes.",
    parameters: {
      type: "object",
      properties: {
        node: {
          type: "string",
          description: "Node name as defined in fleet.yaml",
        },
      },
      required: ["node"],
    },
    execute: async (params: { node: string }) => {
      const result = await binary.getNodeDetail(params.node);
      return JSON.stringify(result, null, 2);
    },
  });

  api.registerTool({
    name: "get_value_gap",
    description:
      "Get the value gap tracker -- situations where Leassh could have taken automated action but the free edition could only observe.",
    parameters: {},
    execute: async () => {
      const result = await binary.getValueGap();
      return JSON.stringify(result, null, 2);
    },
  });
}
