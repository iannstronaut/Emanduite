import { invoke } from "@tauri-apps/api/core";
import type { AppInfo, CommandResponse } from "../contracts/commands";

export async function getAppInfo(): Promise<CommandResponse<AppInfo>> {
  return invoke<CommandResponse<AppInfo>>("get_app_info");
}
