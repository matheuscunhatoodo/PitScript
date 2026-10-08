import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { RecordedMeeting } from "../types/meeting";
import { listMeetings } from "../services/meeting";
import {
  formatDuration,
  formatMeetingDate,
  groupMeetingsByDay,
  meetingProgressLabel,
} from "../utils/meeting";
import { Icon } from "../components/Icon";

export function HomePage({
  onNewRecording,
  onOpenMeeting,
}: {
  onNewRecording: () => void;
  onOpenMeeting: (id: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [meetings, setMeetings] = useState<RecordedMeeting[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    let mounted = true;
    let timer: number | undefined;
    let fetching = false;
    async function load() {
      if (fetching) return;
      fetching = true;
      window.clearTimeout(timer);
      try {
        const saved = await listMeetings();
        if (!mounted) return;
        setMeetings(saved);
        setError(null);
        if (
          saved.some(
            (m) =>
              m.status === "recording" ||
              m.status === "processing" ||
              m.transcriptionStatus === "processing",
          )
        )
          timer = window.setTimeout(() => void load(), 2000);
      } catch {
        if (mounted)
          setError(
            "Não foi possível carregar o histórico. Suas reuniões continuam salvas neste computador.",
          );
      } finally {
        fetching = false;
        if (mounted) setLoading(false);
      }
    }
    void load();
    const subscriptions = isTauri()
      ? Promise.allSettled(
          ["meeting-lifecycle-changed", "transcription-finished"].map((event) =>
            listen(event, () => void load()),
          ),
        )
      : Promise.resolve([]);
    return () => {
      mounted = false;
      window.clearTimeout(timer);
      void subscriptions.then((results) =>
        results.forEach((r) => {
          if (r.status === "fulfilled") r.value();
        }),
      );
    };
  }, [revision]);
  const filtered = meetings.filter((m) =>
    m.title
      .toLocaleLowerCase("pt-BR")
      .includes(query.trim().toLocaleLowerCase("pt-BR")),
  );
  return (
    <div className="page page--home">
      <header className="page-header">
        <div>
          <h1>Biblioteca</h1>
          <p>Suas reuniões, neste computador.</p>
        </div>
        <button className="button button--primary" onClick={onNewRecording}>
          <Icon name="plus" size={21} />
          Nova gravação
        </button>
      </header>
      <label className="search-field">
        <Icon name="search" size={22} />
        <span className="sr-only">Buscar reuniões</span>
        <input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Buscar reuniões"
        />
      </label>
      <section className="meeting-list" aria-label="Reuniões salvas">
        <div className="meeting-list__header" aria-hidden="true">
          <span>Reunião</span>
          <span>Duração</span>
          <span>Transcrição</span>
          <span />
        </div>
        {loading ? (
          <div className="empty-state" role="status">
            Carregando reuniões…
          </div>
        ) : error ? (
          <div className="empty-state" role="alert">
            <strong>{error}</strong>
            <button
              className="button button--outline"
              onClick={() => setRevision((v) => v + 1)}
            >
              Tentar novamente
            </button>
          </div>
        ) : filtered.length ? (
          groupMeetingsByDay(filtered).map((group) => (
            <div key={group.key} className="meeting-day">
              <h2>{group.label}</h2>
              {group.meetings.map((meeting) => {
                const label = meetingProgressLabel(meeting);
                const tone =
                  meeting.status === "failed" ||
                  meeting.status === "failed_partial" ||
                  meeting.transcriptionStatus === "failed"
                    ? "warning"
                    : meeting.status === "recording"
                      ? "recording"
                      : label === "Pronta"
                        ? "ready"
                        : meeting.transcriptionStatus === "processing" ||
                            meeting.status === "processing"
                          ? "processing"
                          : "idle";
                const sources = [
                  meeting.microphoneEnabled && "Microfone",
                  meeting.systemAudioEnabled && "computador",
                  meeting.videoEnabled && "vídeo",
                ]
                  .filter(Boolean)
                  .join(" e ");
                return (
                  <button
                    className="meeting-row"
                    key={meeting.id}
                    onClick={() => onOpenMeeting(meeting.id)}
                    aria-label={`Abrir ${meeting.title}`}
                  >
                    <span className="meeting-row__title">
                      <strong>{meeting.title}</strong>
                      <span className="meeting-row__meta">
                        {formatMeetingDate(meeting.startedAt).time}
                        {meeting.microphoneEnabled && (
                          <Icon name="mic" size={15} />
                        )}
                        {meeting.systemAudioEnabled && (
                          <Icon name="screen" size={16} />
                        )}
                        {meeting.videoEnabled && <Icon name="file" size={15} />}
                        <span>{sources || "Sem fontes"}</span>
                      </span>
                    </span>
                    <span className="meeting-row__duration">
                      {formatDuration(meeting.durationSeconds)}
                    </span>
                    <span className={`meeting-status meeting-status--${tone}`}>
                      <span className="status-dot" />
                      {label}
                    </span>
                    <Icon name="arrow" size={19} />
                  </button>
                );
              })}
            </div>
          ))
        ) : (
          <div className="empty-state">
            <Icon name={query ? "search" : "folder"} size={32} />
            <strong>
              {meetings.length
                ? "Nenhuma reunião encontrada"
                : "Sua primeira reunião começa aqui"}
            </strong>
            <span>
              {meetings.length
                ? "Tente buscar por outro nome."
                : "Grave, ouça e consulte a transcrição neste computador."}
            </span>
            {!meetings.length && (
              <button
                className="button button--primary"
                onClick={onNewRecording}
              >
                Nova gravação
              </button>
            )}
          </div>
        )}
      </section>
      <footer className="library-footer">
        <span>
          {meetings.length}{" "}
          {meetings.length === 1 ? "reunião salva" : "reuniões salvas"}
        </span>
        <button
          className="text-button"
          disabled={loading}
          onClick={() => setRevision((v) => v + 1)}
        >
          Atualizar
        </button>
      </footer>
    </div>
  );
}
