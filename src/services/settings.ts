import { invoke, isTauri } from "@tauri-apps/api/core";

export type AppSettings = {
  microphoneDeviceId: string | null;
  outputDeviceId: string | null;
  transcriptionModel: string;
  language: string;
  maxThreads: number;
  videoResolution: string;
  videoFps: number;
  recordingsDirectory: string | null;
};
export type SettingsInfo = {
  settings: AppSettings;
  effectiveRecordingsDirectory: string;
  models: { id: string; name: string; available: boolean }[];
  warnings: string[];
  busy: boolean;
};
export type AudioDevice = { id: string; name: string; isDefault: boolean };

function desktop() {
  if (!isTauri())
    throw new Error(
      "Abra o aplicativo desktop para acessar as configurações locais.",
    );
}
export function getSettings() {
  desktop();
  return invoke<SettingsInfo>("get_settings");
}
export function saveSettings(settings: AppSettings) {
  desktop();
  return invoke<SettingsInfo>("save_settings", { settings });
}
export function listInputDevices() {
  desktop();
  return invoke<AudioDevice[]>("list_input_devices");
}
export function listOutputDevices() {
  desktop();
  return invoke<AudioDevice[]>("list_output_devices");
}
