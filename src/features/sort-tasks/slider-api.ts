import { invoke } from "@tauri-apps/api/core";
import type { SliderWorkspaceState } from "./types";

interface SliderWorkspaceInput {
  projectPath: string;
  taskId: string;
}

export function loadSliderWorkspace(input: SliderWorkspaceInput): Promise<SliderWorkspaceState> {
  return invoke("load_slider_workspace", { input });
}

export function confirmSliderValue(
  input: SliderWorkspaceInput & { value: number },
): Promise<SliderWorkspaceState> {
  return invoke("confirm_slider_value", { input });
}
