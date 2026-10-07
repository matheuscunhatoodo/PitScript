import { useEffect, useState } from "react";
import type { RecordedMeeting } from "../types/meeting";
import { listMeetings } from "../services/meeting";
import { formatDuration, formatMeetingDate } from "../utils/meeting";
import { Icon } from "../components/Icon";
import { StatusBadge } from "../components/StatusBadge";

type HomePageProps = {
  onNewRecording: () => void;
  onOpenMeeting: (id: string) => void;
};

export function HomePage({ onNewRecording, onOpenMeeting }: HomePageProps) {
  const [query, setQuery] = useState("");
  const [meetings, setMeetings] = useState<RecordedMeeting[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);

  useEffect(() => {
    let mounted = true;
    async function load() {
      setLoading(true);
      setError(null);
      try {
        const saved = await listMeetings();
        if (mounted) setMeetings(saved);
      } catch (cause) {
        if (mounted) setError(String(cause));
      } finally {
        if (mounted) setLoading(false);
      }
    }
    void load();
    return () => {
      mounted = false;
    };
  }, [revision]);
  const filteredMeetings = meetings.filter((meeting) =>
    meeting.title
      .toLocaleLowerCase("pt-BR")
      .includes(query.trim().toLocaleLowerCase("pt-BR")),
  );

  return (
    <div className="page page--home">
      <header className="page-header">
        <div>
          <h1>Suas reuniões</h1>
          <p>Encontre e acompanhe suas gravações em um só lugar.</p>
        </div>
        <button
          className="button button--primary"
          type="button"
          onClick={onNewRecording}
        >
          <Icon name="plus" size={19} />
          Nova gravação
        </button>
      </header>

      <section className="meeting-section" aria-labelledby="meeting-list-title">
        <div className="section-heading">
          <div>
            <h2 id="meeting-list-title">Histórico</h2>
            <p>Reuniões recentes</p>
          </div>
          <div className="history-actions">
            <span className="count">{meetings.length} reuniões</span>
            <button
              className="text-button"
              type="button"
              disabled={loading}
              onClick={() => setRevision((value) => value + 1)}
            >
              Atualizar
            </button>
          </div>
        </div>

        <label className="search-field">
          <Icon name="search" size={20} />
          <span className="sr-only">Buscar reuniões</span>
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Buscar reuniões..."
          />
        </label>

        <div className="meeting-list">
          <div className="meeting-list__header" aria-hidden="true">
            <span>Reunião</span>
            <span>Data</span>
            <span>Duração</span>
            <span>Status</span>
            <span>Fontes</span>
            <span />
          </div>
          {loading ? (
            <div className="empty-state" role="status">
              Carregando reuniões…
            </div>
          ) : error ? (
            <div className="empty-state" role="alert">
              <strong>Não foi possível carregar o histórico</strong>
              <span>{error}</span>
            </div>
          ) : filteredMeetings.length > 0 ? (
            filteredMeetings.map((meeting) => {
              const { date, time } = formatMeetingDate(meeting.startedAt);
              return (
                <div className="meeting-row" key={meeting.id}>
                  <div className="meeting-row__title">
                    <button
                      type="button"
                      onClick={() => onOpenMeeting(meeting.id)}
                    >
                      {meeting.title}
                    </button>
                    <span>Ver detalhes</span>
                  </div>
                  <div className="meeting-row__date">
                    {date}
                    <small>{time}</small>
                  </div>
                  <span className="meeting-row__duration">
                    {formatDuration(meeting.durationSeconds)}
                  </span>
                  <StatusBadge status={meeting.status} />
                  <div className="source-icons">
                    {(meeting.microphoneEnabled ||
                      meeting.systemAudioEnabled) && (
                      <span title="Áudio" aria-label="Áudio">
                        <Icon name="speaker" size={18} />
                      </span>
                    )}
                    {meeting.videoEnabled && (
                      <span title="Vídeo" aria-label="Vídeo">
                        <Icon name="screen" size={18} />
                      </span>
                    )}
                  </div>
                  <button
                    className="meeting-row__open"
                    type="button"
                    aria-label={`Abrir ${meeting.title}`}
                    onClick={() => onOpenMeeting(meeting.id)}
                  >
                    <Icon name="arrow" size={18} />
                  </button>
                </div>
              );
            })
          ) : (
            <div className="empty-state">
              <Icon name="search" size={26} />
              <strong>
                {meetings.length
                  ? "Nenhuma reunião encontrada"
                  : "Você ainda não tem reuniões salvas"}
              </strong>
              <span>
                {meetings.length
                  ? "Tente buscar por outro nome."
                  : "Inicie uma gravação para criar sua primeira reunião."}
              </span>
            </div>
          )}
        </div>
      </section>
    </div>
  );
}
