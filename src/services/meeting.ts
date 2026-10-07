import { convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";
import type { RecordedMeeting, DiarizationState } from "../types/meeting";

function requireDesktop() {
  if (!isTauri())
    throw new Error(
      "Abra o aplicativo desktop para acessar as reuniões salvas.",
    );
}

export function listMeetings() {
  requireDesktop();
  return invoke<RecordedMeeting[]>("list_meetings");
}

export function getMeeting(id: string) {
  requireDesktop();
  return invoke<RecordedMeeting | null>("get_meeting", { id });
}

export function getMeetingDiarization(id: string) {
  requireDesktop();
  return invoke<DiarizationState>("get_meeting_diarization", { id });
}
export function diarizeMeeting(id: string, threads?: number) {
  requireDesktop();
  return invoke<DiarizationState>("diarize_meeting", { id, threads });
}

export async function getMeetingAudio(id: string): Promise<string | null> {
  requireDesktop();
  const path = await invoke<string | null>("get_meeting_audio", { id });
  return path ? convertFileSrc(path) : null;
}

export function exportTranscript(id: string, traditional = false) {
  requireDesktop();
  return invoke<boolean>("export_transcript", { id, traditional });
}

export function openMeetingFolder(id: string) {
  requireDesktop();
  return invoke<void>("open_meeting_folder", { id });
}

export async function copyTranscript(text: string) {
  if (!navigator.clipboard)
    throw new Error("A área de transferência não está disponível.");
  await navigator.clipboard.writeText(text);
}

export type StartMeetingInput = {
  title: string;
  microphoneEnabled: boolean;
  systemAudioEnabled: boolean;
  videoEnabled: boolean;
  microphoneDeviceId?: string;
  outputDeviceId?: string;
  videoSourceId?: string;
};

export type VideoSource = {
  id: string;
  title: string;
  kind: "window" | "monitor";
};
export type VideoSourceState = {
  status: string;
  framesWritten: number;
  durationMs: number;
  encoder: string;
  hardwareAccelerated: boolean;
  finalized: boolean;
  error: string | null;
};
export function listVideoSources() {
  requireDesktop();
  return invoke<VideoSource[]>("list_video_sources");
}

export type AudioSourceState = {
  meetingId: string | null;
  status: string;
  bytesWritten: number;
  error: string | null;
};

export type MeetingRecordingState = {
  active: boolean;
  meeting: {
    id: string;
    title: string;
    status: string;
    durationSeconds: number;
    transcriptionStatus: string;
  } | null;
  microphone: AudioSourceState | null;
  system: AudioSourceState | null;
  video: VideoSourceState | null;
  errors: string[];
  warnings?: string[];
};

export type TranscriptionState = {
  meetingId: string;
  status: "pending" | "processing" | "completed" | "failed" | "cancelled";
  progress: number;
  error: string | null;
  threads: number;
  stage?: string;
};

export function startMeeting(input: StartMeetingInput) {
  return invoke<MeetingRecordingState>("start_meeting", { input });
}

export function stopMeeting(threads?: number) {
  return invoke<MeetingRecordingState>("stop_meeting", { threads });
}

export function getRecordingState() {
  return invoke<MeetingRecordingState>("get_recording_state");
}

export function transcribeMeeting(id: string, threads?: number) {
  return invoke<TranscriptionState>("transcribe_meeting", { id, threads });
}

export function cancelTranscription(id: string) {
  return invoke<TranscriptionState>("cancel_transcription", { id });
}

export function getTranscriptionState(id: string) {
  return invoke<TranscriptionState>("get_transcription_state", { id });
}
