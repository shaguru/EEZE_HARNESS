import { invoke } from "@tauri-apps/api/core";
import type { ThreadItem } from "../generated/ts/v2/ThreadItem";
export type Config = {
  schemaVersion: number;
  projects: string[];
  selectedProject: string | null;
  selectedThread: string | null;
  binaryPath: string | null;
  model: string | null;
  theme: string;
  fontSize: number;
  sandbox: string;
  approvalPolicy: string;
  reasoningEffort: string | null;
  reasoningSummary: string;
  serviceTier: string | null;
  layout: "bottom" | "left" | "right" | string;
  sidebarWidth: number;
  inspectorWidth: number;
  labels: Record<string, string>;
  debugLog: boolean;
};
export type Approval = {
  key: string;
  id: string | number;
  method: string;
  params: Record<string, any>;
  supported: boolean;
};
export type Snapshot = {
  lifecycle: string;
  error: string | null;
  warning: string | null;
  config: Config;
  root: string;
  binary: { path: string; version: string; sha256: string } | null;
  models: any[];
  account: any;
  login: any;
  threadId: string | null;
  turnId: string | null;
  threads: any[];
  items: (ThreadItem & Record<string, any>)[];
  approvals: Approval[];
  diff: string;
  capabilities: string[];
  threadCursor: string | null;
  historyCursor: string | null;
  generation: number;
};
export const call = <T = void>(
  command: string,
  args?: Record<string, unknown>,
) => invoke<T>(command, args);
