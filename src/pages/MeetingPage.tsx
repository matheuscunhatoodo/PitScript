import { useDeferredValue, useEffect, useMemo, useState } from "react";
import { Icon } from "../components/Icon";
import { AudioPlayer } from "../components/AudioPlayer";
import { TranscriptView } from "../components/TranscriptView";
import {
  copyTranscript,
  exportTranscript,
  getMeeting,
  getMeetingAudio,
  getMeetingVideo,
  openMeetingFolder,
  getMeetingDiarization,
  diarizeMeeting,
  cancelTranscription,
  transcribeMeeting,
} from "../services/meeting";
import type { RecordedMeeting, DiarizationState } from "../types/meeting";
import {
  findTranscriptMatches,
  formatDuration,
  formatMeetingDate,
  formatSegmentTranscript,
  buildTranscriptBlocks,
  meetingProgressLabel,
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
  const [videoUrl, setVideoUrl] = useState<string | null>(null);
  const [videoError, setVideoError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const deferredQuery = useDeferredValue(query);
  const [matchIndex, setMatchIndex] = useState(0);
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const [currentTime, setCurrentTime] = useState(0);
  const [seekRequest, setSeekRequest] = useState<{
    seconds: number;
    revision: number;
  } | null>(null);
  const text = useMemo(
    () =>
      diarization?.segments.length && !showTraditional
        ? formatSegmentTranscript(diarization.segments)
        : (meeting?.transcription ?? ""),
    [diarization?.segments, meeting?.transcription, showTraditional],
  );
  const visibleSegments = useMemo(
    () => (!showTraditional ? (diarization?.segments ?? []) : []),
    [showTraditional, diarization?.segments],
  );
  const searchText = useMemo(
    () =>
      visibleSegments.length
        ? buildTranscriptBlocks(visibleSegments)
            .map((block) => block.searchText)
            .join("\n\n")
        : text,
    [visibleSegments, text],
  );
  const hasTranscript =
    meeting?.transcriptionStatus === "completed" && !!text.trim();
  const matches = useMemo(
    () => findTranscriptMatches(searchText, deferredQuery),
    [searchText, deferredQuery],
  );
  const selectedMatch = matches.length ? matchIndex % matches.length : 0;

  useEffect(() => {
    let mounted = true;
    async function load() {
      setLoading(true);
      setCurrentTime(0);
      setSeekRequest(null);
      setLoadError(null);
      setAudioError(null);
      setAudioUrl(null);
      setVideoError(null);
      setVideoUrl(null);
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
        (async () => {
          try {
            const url = await getMeetingVideo(id);
            if (mounted) setVideoUrl(url);
          } catch (cause) {
            if (mounted) setVideoError(String(cause));
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
        const [saved, identified] = await Promise.allSettled([
          getMeeting(id),
          getMeetingDiarization(id),
        ]);
        if (mounted) {
          if (saved.status === "fulfilled" && saved.value)
            setMeeting(saved.value);
          if (identified.status === "fulfilled")
            setDiarization(identified.value);
          else
            setDiarizationError(
              "Não foi possível atualizar os segmentos. A transcrição tradicional continua disponível.",
            );
          if (saved.status === "rejected")
            setActionError(
              "Não foi possível atualizar a transcrição. Tentaremos novamente.",
            );
          if (
            saved.status === "rejected" ||
            saved.value?.transcriptionStatus === "processing" ||
            (identified.status === "fulfilled" &&
              ["processing", "pending"].includes(identified.value.status))
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
      <button className="text-button back-button" onClick={onBack}>
        <Icon name="back" size={19} />
        Voltar à biblioteca
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
                  {date?.date} · {date?.time}
                </span>
                <span>{formatDuration(meeting.durationSeconds)}</span>
                <span
                  className={
                    hasTranscript
                      ? "meeting-status meeting-status--ready"
                      : "meeting-status meeting-status--idle"
                  }
                >
                  <span className="status-dot" />
                  {hasTranscript
                    ? "Transcrição pronta"
                    : meetingProgressLabel(meeting)}
                </span>
              </div>
            </div>
            <button
              className="button button--outline"
              disabled={busy}
              onClick={() =>
                void runAction(async () => {
                  await openMeetingFolder(id);
                  return "Pasta aberta no Explorador de Arquivos.";
                })
              }
            >
              <Icon name="folder" size={20} />
              Abrir pasta
            </button>
          </header>
          {audioError && (
            <p className="inline-notice" role="alert">
              {audioError}
            </p>
          )}
          {["failed", "failed_partial"].includes(meeting.status) && (
            <p className="inline-notice" role="alert">
              A gravação teve uma falha. O áudio e o texto disponíveis foram
              preservados. Consulte os arquivos na pasta da reunião.
            </p>
          )}
          {!audioUrl && (
            <p className="panel-help">
              Nenhum áudio reproduzível foi encontrado. Consulte a pasta da
              reunião.
            </p>
          )}
          {videoError && (
            <p className="inline-notice" role="alert">
              {videoError}
            </p>
          )}
          {meeting.videoEnabled && !videoUrl && !videoError && (
            <p className="panel-help">
              Nenhum vídeo reproduzível foi encontrado. Consulte o MP4 na pasta
              da reunião.
            </p>
          )}
          {(audioUrl || videoUrl) && (
            <AudioPlayer
              key={`${audioUrl}:${videoUrl}`}
              url={audioUrl}
              videoUrl={videoUrl}
              onTimeChange={setCurrentTime}
              seekRequest={seekRequest}
            />
          )}
          <section aria-labelledby="transcript-title">
            <div className="transcript-toolbar">
              <h2 id="transcript-title">Transcrição</h2>
              <div className="transcript-actions">
                <button
                  className="button button--quiet"
                  disabled={busy || !hasTranscript}
                  onClick={() =>
                    void runAction(async () => {
                      await copyTranscript(text);
                      return "Texto copiado.";
                    })
                  }
                >
                  <Icon name="copy" size={19} />
                  Copiar texto
                </button>
                <button
                  className="button button--primary"
                  disabled={busy || !hasTranscript}
                  onClick={() =>
                    void runAction(async () =>
                      (await exportTranscript(id, showTraditional))
                        ? "TXT exportado."
                        : "Exportação cancelada.",
                    )
                  }
                >
                  <Icon name="file" size={19} />
                  Exportar TXT
                </button>
              </div>
            </div>
            {(diarization?.error || diarizationError) && (
              <div className="inline-notice" role="alert">
                <strong>O texto tradicional continua disponível.</strong>
                <details>
                  <summary>Detalhes da identificação de participantes</summary>
                  {diarization?.error ?? diarizationError}
                </details>
              </div>
            )}
            {processing && (
              <div
                className="inline-notice inline-notice--transcription"
                role="status"
              >
                <span>
                  {diarization?.stage === "diarization"
                    ? "Identificando participantes"
                    : diarization?.stage === "source_transcription"
                      ? "Preparando texto por fonte"
                      : "Transcrevendo localmente"}
                  {diarization?.status === "processing"
                    ? " · " + diarization.progress + "%"
                    : ""}
                </span>
                {diarization?.status === "processing" && (
                  <progress
                    value={diarization.progress}
                    max={100}
                    aria-label="Progresso da identificação de participantes"
                  />
                )}
                <button
                  className="text-button"
                  disabled={busy}
                  onClick={() =>
                    void runAction(async () => {
                      await cancelTranscription(id);
                      return "Cancelamento solicitado. O áudio e o texto já salvo serão preservados.";
                    })
                  }
                >
                  Cancelar processamento
                </button>
              </div>
            )}
            {hasTranscript ? (
              <>
                <div className="search-field transcript-search">
                  <Icon name="search" size={22} />
                  <label className="sr-only" htmlFor="transcript-search">
                    Pesquisar na transcrição
                  </label>
                  <input
                    id="transcript-search"
                    type="search"
                    value={query}
                    onChange={(event) => {
                      setQuery(event.target.value);
                      setMatchIndex(0);
                    }}
                    placeholder="Pesquisar na transcrição"
                  />
                  {deferredQuery.trim() && (
                    <div className="transcript-search-navigation">
                      <span role="status">
                        {matches.length
                          ? selectedMatch + 1 + " de " + matches.length
                          : "Nenhum resultado"}
                      </span>
                      <button
                        className="icon-button icon-button--previous"
                        aria-label="Resultado anterior"
                        disabled={!matches.length}
                        onClick={() =>
                          setMatchIndex(
                            (selectedMatch + matches.length - 1) %
                              matches.length,
                          )
                        }
                      >
                        <Icon name="arrow" size={19} />
                      </button>
                      <button
                        className="icon-button"
                        aria-label="Próximo resultado"
                        disabled={!matches.length}
                        onClick={() =>
                          setMatchIndex((selectedMatch + 1) % matches.length)
                        }
                      >
                        <Icon name="arrow" size={19} />
                      </button>
                    </div>
                  )}
                </div>
                <TranscriptView
                  segments={visibleSegments}
                  text={text}
                  matches={matches}
                  selectedMatch={selectedMatch}
                  currentTime={currentTime}
                  onSeek={
                    audioUrl || videoUrl
                      ? (seconds) =>
                          setSeekRequest((request) => ({
                            seconds,
                            revision: (request?.revision ?? 0) + 1,
                          }))
                      : undefined
                  }
                />
              </>
            ) : (
              <div className="transcript-empty">
                <Icon name="file" size={27} />
                <strong>Transcrição ainda não disponível</strong>
                <span>{transcriptMessages[meeting.transcriptionStatus]}</span>
                {!processing && (
                  <button
                    className="button button--outline"
                    disabled={busy}
                    onClick={() =>
                      void runAction(async () => {
                        await transcribeMeeting(id);
                        setRevision((value) => value + 1);
                        return "Transcrição solicitada localmente.";
                      })
                    }
                  >
                    Transcrever áudio salvo
                  </button>
                )}
              </div>
            )}
          </section>
          {!!diarization?.segments.length && (
            <div className="transcript-options">
              <button
                className="text-button"
                onClick={() => {
                  setShowTraditional((value) => !value);
                  setMatchIndex(0);
                }}
              >
                {showTraditional
                  ? "Ver participantes"
                  : "Ver texto tradicional"}
              </button>
              <span>
                Locutores estimados. Trechos incertos usam “Participantes”.
              </span>
            </div>
          )}
          <footer className="detail-footer">
            {meeting.transcriptionStatus === "completed" &&
              !processing &&
              diarization?.status !== "completed" && (
                <button
                  className="text-button"
                  disabled={busy}
                  onClick={() =>
                    void runAction(async () => {
                      setDiarization(await diarizeMeeting(id));
                      return "Identificação de participantes iniciada localmente.";
                    })
                  }
                >
                  Identificar participantes
                </button>
              )}
            <button
              className="text-button"
              disabled={busy}
              onClick={() => setRevision((value) => value + 1)}
            >
              Atualizar reunião
            </button>
          </footer>
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
    </div>
  );
}
