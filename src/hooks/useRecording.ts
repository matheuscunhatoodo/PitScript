import { useCallback, useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  getRecordingState,
  getTranscriptionState,
  startMeeting,
  stopMeeting,
  type MeetingRecordingState,
  type StartMeetingInput,
  type TranscriptionState,
} from "../services/meeting";

export function useRecording() {
  const [capture, setCapture] = useState<MeetingRecordingState | null>(null);
  const [transcription, setTranscription] = useState<TranscriptionState | null>(
    null,
  );
  const [busy, setBusy] = useState(false);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const operation = useRef(false);
  const lifecycle = useRef(0);
  const alive = useRef(false);
  const pending = useRef<Promise<void> | null>(null);
  const queued = useRef(false);
  const refresh = useCallback(() => {
    if (!isTauri()) return Promise.resolve();
    queued.current = true;
    if (pending.current) return pending.current;
    pending.current = (async () => {
      do {
        queued.current = false;
        const generation = lifecycle.current;
        try {
          const state = await getRecordingState();
          if (generation !== lifecycle.current) {
            queued.current = alive.current;
            continue;
          }
          setCapture(state);
          setReady(true);
          setError(null);
          if (!state.active && state.meeting) {
            try {
              const progress = await getTranscriptionState(state.meeting.id);
              if (generation === lifecycle.current) setTranscription(progress);
            } catch {
              // Capture state remains usable when progress is temporarily unavailable.
            }
          } else setTranscription(null);
        } catch {
          if (generation === lifecycle.current)
            setError(
              "Não foi possível atualizar o estado. A captura existente não foi interrompida. Tente atualizar novamente.",
            );
        }
      } while (queued.current && alive.current);
    })().finally(() => {
      pending.current = null;
    });
    return pending.current;
  }, []);

  useEffect(() => {
    lifecycle.current += 1;
    alive.current = true;
    void refresh();
    const subscriptions = isTauri()
      ? Promise.allSettled(
          [
            "meeting-lifecycle-changed",
            "recording-status",
            "system-recording-status",
            "video-recording-status",
            "transcription-finished",
          ].map((name) => listen(name, () => void refresh())),
        )
      : Promise.resolve([]);
    return () => {
      alive.current = false;
      lifecycle.current += 1;
      queued.current = false;
      void subscriptions.then((results) =>
        results.forEach((result) => {
          if (result.status === "fulfilled") result.value();
        }),
      );
    };
  }, [refresh]);

  useEffect(() => {
    if (!capture?.active && transcription?.status !== "processing") return;
    const timer = window.setInterval(() => void refresh(), 1000);
    return () => window.clearInterval(timer);
  }, [capture?.active, transcription?.status, refresh]);

  async function start(input: StartMeetingInput) {
    if (
      operation.current ||
      !ready ||
      capture?.active ||
      transcription?.status === "processing"
    )
      return;
    operation.current = true;
    setBusy(true);
    setActionError(null);
    try {
      setCapture(await startMeeting(input));
      setTranscription(null);
    } catch (cause) {
      setActionError(`Não foi possível iniciar a gravação. ${String(cause)}`);
    } finally {
      operation.current = false;
      setBusy(false);
      await refresh();
    }
  }

  async function stop() {
    if (operation.current || !capture?.active) return null;
    operation.current = true;
    setBusy(true);
    setActionError(null);
    try {
      const state = await stopMeeting();
      setCapture(state);
      return state;
    } catch (cause) {
      setActionError(
        `Não foi possível finalizar. Tente novamente; os arquivos válidos serão preservados. ${String(cause)}`,
      );
      return null;
    } finally {
      operation.current = false;
      setBusy(false);
      await refresh();
    }
  }
  return {
    capture,
    transcription,
    busy,
    ready,
    error: actionError ?? error,
    refresh,
    start,
    stop,
  };
}
export type RecordingController = ReturnType<typeof useRecording>;
