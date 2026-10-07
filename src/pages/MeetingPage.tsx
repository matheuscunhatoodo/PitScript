import {
  Fragment,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Icon } from "../components/Icon";
import { StatusBadge } from "../components/StatusBadge";
import {
  copyTranscript,
  exportTranscript,
  getMeeting,
  getMeetingAudio,
  openMeetingFolder,
  getMeetingDiarization,
  diarizeMeeting,
  cancelTranscription,
} from "../services/meeting";
import type { RecordedMeeting, DiarizationState } from "../types/meeting";
import {
  findTranscriptMatches,
  formatDuration,
  formatMeetingDate,
  formatSegmentTranscript,
} from "../utils/meeting";

const transcriptMessages: Record<
  RecordedMeeting["transcriptionStatus"],
  string
> = {
  pending: "A transcrição ainda está pendente.",
  processing:
    "Transcrição em andamento. O texto aparecerá quando estiver pronto.",
  completed: "A transcrição foi concluída sem texto reconhecido.",
  failed: "A transcrição falhou. Os arquivos de áudio foram preservados.",
  cancelled:
    "A transcrição foi cancelada. Os arquivos de áudio foram preservados.",
};

export function MeetingPage({
  id,
  onBack,
}: {
  id: string;
  onBack: () => void;
}) {
  const [meeting, setMeeting] = useState<RecordedMeeting | null>(null);
  const [diarization, setDiarization] = useState<DiarizationState | null>(null);
  const [diarizationError, setDiarizationError] = useState<string | null>(null);
  const [showTraditional, setShowTraditional] = useState(false);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [audioUrl, setAudioUrl] = useState<string | null>(null);
  const [audioError, setAudioError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const deferredQuery = useDeferredValue(query);
  const [matchIndex, setMatchIndex] = useState(0);
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const activeMatch = useRef<HTMLElement | null>(null);
  const text = useMemo(
    () =>
      diarization?.segments.length && !showTraditional
        ? formatSegmentTranscript(diarization.segments)
        : (meeting?.transcription ?? ""),
    [diarization?.segments, meeting?.transcription, showTraditional],
  );
  const hasTranscript =
    meeting?.transcriptionStatus === "completed" && !!text.trim();
  const matches = useMemo(
    () => findTranscriptMatches(text, deferredQuery),
    [text, deferredQuery],
  );
  const selectedMatch = matches.length ? matchIndex % matches.length : 0;

  useEffect(() => {
    let mounted = true;
    async function load() {
      setLoading(true);
      setLoadError(null);
      setAudioError(null);
      setAudioUrl(null);
      setDiarizationError(null);
      setDiarization(null);
      setShowTraditional(false);
      await Promise.all([
        (async () => {
          try {
            const saved = await getMeeting(id);
            if (mounted) {
              if (!saved) setLoadError("A reunião não foi encontrada.");
              setMeeting(saved);
            }
          } catch (cause) {
            if (mounted) setLoadError(String(cause));
          }
        })(),
        (async () => {
          try {
            const state = await getMeetingDiarization(id);
            if (mounted) setDiarization(state);
          } catch (cause) {
            if (mounted)
              setDiarizationError(
                `Não foi possível consultar os segmentos: ${String(cause)}`,
              );
          }
        })(),
        (async () => {
          try {
            const url = await getMeetingAudio(id);
            if (mounted) setAudioUrl(url);
          } catch (cause) {
            if (mounted) setAudioError(String(cause));
          }
        })(),
      ]);
      if (mounted) setLoading(false);
    }
    void load();
    return () => {
      mounted = false;
    };
  }, [id, revision]);

  const processing =
    meeting?.transcriptionStatus === "processing" ||
    diarization?.status === "processing" ||
    diarization?.status === "pending";
  useEffect(() => {
    if (!processing) return;
    let mounted = true;
    let timer: number;
    async function refresh() {
      try {
        const [saved, identified] = await Promise.all([
          getMeeting(id),
          getMeetingDiarization(id),
        ]);
        if (mounted && saved) {
          setMeeting(saved);
          setDiarization(identified);
          if (
            saved.transcriptionStatus === "processing" ||
            identified.status === "processing" ||
            identified.status === "pending"
          )
            timer = window.setTimeout(() => void refresh(), 2000);
        }
      } catch (cause) {
        if (mounted)
          setActionError(
            `Falha ao atualizar transcrição: ${String(cause)}. Use Atualizar reunião para tentar novamente.`,
          );
      }
    }
    timer = window.setTimeout(() => void refresh(), 2000);
    return () => {
      mounted = false;
      window.clearTimeout(timer);
    };
  }, [id, processing, revision]);

  useEffect(() => {
    activeMatch.current?.scrollIntoView({ block: "nearest" });
  }, [selectedMatch, deferredQuery, text]);

  async function runAction(action: () => Promise<string>) {
    if (busy) return;
    setBusy(true);
    setFeedback(null);
    setActionError(null);
    try {
      setFeedback(await action());
    } catch (cause) {
      setActionError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  const date = meeting ? formatMeetingDate(meeting.startedAt) : null;
  return (
    <div className="page page--detail">
      <button
        className="text-button back-button"
        type="button"
        onClick={onBack}
      >
        <Icon name="back" size={17} /> Voltar para reuniões
      </button>
      {loading ? (
        <p role="status">Carregando reunião…</p>
      ) : loadError || !meeting ? (
        <div className="inline-notice" role="alert">
          {loadError ?? "Reunião indisponível."}
        </div>
      ) : (
        <>
          <header className="page-header page-header--detail">
            <div>
              <h1>{meeting.title}</h1>
              <div className="detail-meta">
                <span>
                  <Icon name="calendar" size={17} /> {date?.date} às{" "}
                  {date?.time}
                </span>
                <span>
                  <Icon name="clock" size={17} />{" "}
                  {formatDuration(meeting.durationSeconds)}
                </span>
                <StatusBadge status={meeting.status} />
              </div>
            </div>
          </header>
          <div className="detail-layout">
            <section
              className="detail-panel audio-panel"
              aria-labelledby="audio-title"
            >
              <div className="panel-title">
                <span className="panel-title__icon">
                  <Icon name="speaker" size={20} />
                </span>
                <div>
                  <h2 id="audio-title">Áudio da reunião</h2>
                  <p>Reprodução local</p>
                </div>
              </div>
              {audioUrl ? (
                <audio
                  className="meeting-audio"
                  controls
                  preload="metadata"
                  src={audioUrl}
                  aria-label="Áudio da reunião"
                  onError={() =>
                    setAudioError(
                      "Não foi possível reproduzir o áudio. Verifique os arquivos na pasta da reunião.",
                    )
                  }
                />
              ) : (
                <p className="panel-help">
                  {audioError ??
                    "Nenhum arquivo de áudio válido foi encontrado para esta reunião."}
                </p>
              )}
              {audioUrl && audioError && (
                <p role="alert" className="panel-help">
                  {audioError}
                </p>
              )}
              <div className="source-summary">
                <h3>Fontes habilitadas na gravação</h3>
                <span
                  className={
                    meeting.microphoneEnabled ? "" : "source-summary__off"
                  }
                >
                  <Icon name="mic" size={17} /> Microfone
                </span>
                <span
                  className={
                    meeting.systemAudioEnabled ? "" : "source-summary__off"
                  }
                >
                  <Icon name="speaker" size={17} /> Áudio do computador
                </span>
              </div>
              <button
                className="button button--outline"
                type="button"
                disabled={busy}
                onClick={() =>
                  void runAction(async () => {
                    await openMeetingFolder(id);
                    return "Pasta aberta no Explorador de Arquivos.";
                  })
                }
              >
                Abrir pasta da reunião
              </button>
            </section>
            <section
              className="detail-panel transcript-panel"
              aria-labelledby="transcript-title"
            >
              <div className="panel-title panel-title--between">
                <div className="panel-title__group">
                  <span className="panel-title__icon">
                    <Icon name="file" size={20} />
                  </span>
                  <div>
                    <h2 id="transcript-title">Transcrição</h2>
                    <p>Texto salvo localmente</p>
                  </div>
                </div>
                <div className="transcript-actions">
                  <button
                    className="button button--quiet"
                    type="button"
                    disabled={!hasTranscript || busy}
                    onClick={() =>
                      void runAction(async () => {
                        await copyTranscript(text);
                        return "Transcrição copiada.";
                      })
                    }
                  >
                    Copiar texto
                  </button>
                  <button
                    className="button button--quiet"
                    type="button"
                    disabled={!hasTranscript || busy}
                    onClick={() =>
                      void runAction(async () =>
                        (await exportTranscript(id, showTraditional))
                          ? "TXT exportado."
                          : "Exportação cancelada.",
                      )
                    }
                  >
                    <Icon name="download" size={17} /> Exportar TXT
                  </button>
                </div>
              </div>
              <div className="diarization-status">
                {!!diarization?.segments.length && (
                  <button
                    className="text-button"
                    type="button"
                    onClick={() => {
                      setShowTraditional((value) => !value);
                      setMatchIndex(0);
                    }}
                  >
                    {showTraditional
                      ? "Ver participantes"
                      : "Ver texto tradicional"}
                  </button>
                )}
                {diarization?.status === "completed" && (
                  <p className="panel-help">
                    Locutores estimados por reunião. Trechos incertos usam
                    “Participantes”.
                  </p>
                )}
                {(diarization?.status === "processing" ||
                  diarization?.status === "pending") && (
                  <div
                    className="inline-notice inline-notice--transcription"
                    role="status"
                  >
                    <span>
                      {diarization.status === "pending"
                        ? "Aguardando a transcrição tradicional"
                        : diarization.stage === "source_transcription"
                          ? "Preparando texto por fonte"
                          : diarization.stage === "diarization"
                            ? "Identificando participantes"
                            : "Preparando e alinhando segmentos"}{" "}
                      · {diarization.progress}%
                    </span>
                    <progress
                      value={diarization.progress}
                      max={100}
                      aria-label="Progresso da identificação de participantes"
                    />
                    <button
                      className="button button--outline"
                      type="button"
                      disabled={busy}
                      onClick={() =>
                        void runAction(async () => {
                          await cancelTranscription(id);
                          return "Cancelamento solicitado. O texto já salvo será preservado.";
                        })
                      }
                    >
                      Cancelar processamento
                    </button>
                  </div>
                )}
                {(diarization?.error || diarizationError) && (
                  <p className="inline-notice" role="alert">
                    {diarization?.error ?? diarizationError}
                  </p>
                )}
                {meeting.transcriptionStatus === "completed" &&
                  !processing &&
                  diarization?.status !== "completed" && (
                    <button
                      className="button button--outline"
                      type="button"
                      disabled={busy}
                      onClick={() =>
                        void runAction(async () => {
                          setDiarization(await diarizeMeeting(id));
                          return "Identificação de participantes iniciada localmente.";
                        })
                      }
                    >
                      {diarization?.status === "failed" ||
                      diarization?.status === "cancelled"
                        ? "Tentar identificar participantes novamente"
                        : "Identificar participantes"}
                    </button>
                  )}
              </div>
              {hasTranscript ? (
                <>
                  <label className="search-field">
                    <Icon name="search" size={20} />
                    <span className="sr-only">Pesquisar na transcrição</span>
                    <input
                      type="search"
                      value={query}
                      onChange={(event) => {
                        setQuery(event.target.value);
                        setMatchIndex(0);
                      }}
                      placeholder="Pesquisar na transcrição…"
                    />
                  </label>
                  {deferredQuery.trim() && (
                    <div className="transcript-search-navigation">
                      <span role="status">
                        {matches.length
                          ? `${selectedMatch + 1} de ${matches.length} resultados`
                          : "Nenhum resultado"}
                      </span>
                      <button
                        type="button"
                        className="text-button"
                        disabled={!matches.length}
                        onClick={() =>
                          setMatchIndex(
                            (selectedMatch + matches.length - 1) %
                              matches.length,
                          )
                        }
                      >
                        Anterior
                      </button>
                      <button
                        type="button"
                        className="text-button"
                        disabled={!matches.length}
                        onClick={() =>
                          setMatchIndex((selectedMatch + 1) % matches.length)
                        }
                      >
                        Próximo
                      </button>
                    </div>
                  )}
                  <div
                    className="transcript-text"
                    tabIndex={0}
                    aria-label="Texto da transcrição"
                  >
                    {matches.length ? (
                      <>
                        {matches.map((match, index) => (
                          <Fragment key={match.start}>
                            {text.slice(
                              index ? matches[index - 1].end : 0,
                              match.start,
                            )}
                            <mark
                              ref={
                                index === selectedMatch
                                  ? activeMatch
                                  : undefined
                              }
                              className={
                                index === selectedMatch
                                  ? "transcript-match--active"
                                  : ""
                              }
                            >
                              {text.slice(match.start, match.end)}
                            </mark>
                          </Fragment>
                        ))}
                        {text.slice(matches[matches.length - 1].end)}
                      </>
                    ) : (
                      text
                    )}
                  </div>
                </>
              ) : (
                <div className="transcript-empty">
                  <Icon name="file" size={25} />
                  <strong>Transcrição ainda não disponível</strong>
                  <span>{transcriptMessages[meeting.transcriptionStatus]}</span>
                </div>
              )}
            </section>
          </div>
        </>
      )}
      {feedback && (
        <p className="save-feedback" role="status">
          {feedback}
        </p>
      )}
      {actionError && (
        <p className="inline-notice" role="alert">
          {actionError}
        </p>
      )}
      <div className="detail-footer">
        <button
          className="text-button"
          type="button"
          disabled={loading || busy}
          onClick={() => setRevision((value) => value + 1)}
        >
          Atualizar reunião
        </button>
      </div>
    </div>
  );
}
