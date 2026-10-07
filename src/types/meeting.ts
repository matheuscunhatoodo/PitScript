export type MeetingStatus =
  | "created"
  | "recording"
  | "completed"
  | "processing"
  | "failed_partial"
  | "failed";

export type TranscriptSegment = {
  id: string;
  meetingId: string;
  diarizationLabel: string;
  startMs: number;
  endMs: number;
  text: string;
  confidence: number | null;
  source: "microphone" | "system";
  createdAt: string;
};
export type DiarizationState = {
  meetingId: string;
  status:
    | "not_started"
    | "pending"
    | "processing"
    | "completed"
    | "failed"
    | "cancelled";
  stage: string;
  progress: number;
  error: string | null;
  model: string | null;
  segments: TranscriptSegment[];
};

/** Contract returned by the existing SQLite/Tauri meeting APIs. */
export type RecordedMeeting = {
  id: string;
  title: string;
  startedAt: string;
  finishedAt: string | null;
  durationSeconds: number;
  microphoneEnabled: boolean;
  systemAudioEnabled: boolean;
  videoEnabled: boolean;
  microphonePath: string | null;
  systemAudioPath: string | null;
  mergedAudioPath: string | null;
  videoPath: string | null;
  transcription: string | null;
  transcriptionStatus:
    "pending" | "processing" | "completed" | "failed" | "cancelled";
  transcriptionModel: string | null;
  language: string;
  status: MeetingStatus;
  recordingStatus: string | null;
  createdAt: string;
  updatedAt: string;
};

export type Meeting = {
  id: string;
  title: string;
  date: string;
  time: string;
  duration: string;
  status: MeetingStatus;
  microphone: boolean;
  systemAudio: boolean;
  video: boolean;
  transcript: string[];
};
