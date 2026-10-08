import type { RecordingController } from "../hooks/useRecording";
import { formatDuration } from "../utils/meeting";
import { Icon } from "./Icon";

export function RecordingBar({
  recording,
  onShow,
  onOpenMeeting,
  showStop,
}: {
  recording: RecordingController;
  onShow: () => void;
  onOpenMeeting: (id: string) => void;
  showStop: boolean;
}) {
  const { capture, busy, stop } = recording;
  if (!capture?.active) return null;
  return (
    <aside className="recording-bar" aria-label="Gravação em andamento">
      <button className="recording-bar__title" onClick={onShow}>
        <span className="record-dot" />
        <span>{capture.meeting?.title ?? "Gravação em andamento"}</span>
      </button>
      <span className="recording-bar__time">
        {formatDuration(
          capture.elapsedSeconds ?? capture.meeting?.durationSeconds ?? 0,
        )}
      </span>
      <span className="recording-bar__saved">
        {capture.errors.length
          ? "Uma fonte precisa de atenção"
          : "Salvando neste computador"}
      </span>
      {showStop && (
        <button
          className="button button--danger button--compact"
          aria-label="Finalizar gravação"
          disabled={busy}
          onClick={() =>
            void stop().then((state) => {
              if (state?.meeting && !state.active)
                onOpenMeeting(state.meeting.id);
            })
          }
        >
          <Icon name="stop" size={14} />
          {busy ? "Finalizando…" : "Finalizar"}
        </button>
      )}
    </aside>
  );
}
