import { useEffect, useState, type FormEvent } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Icon } from "../components/Icon";
import { getSettings, type AppSettings } from "../services/settings";
import {
  cancelTranscription,
  getRecordingState,
  getTranscriptionState,
  listVideoSources,
  startMeeting,
  stopMeeting,
  transcribeMeeting,
  type MeetingRecordingState,
  type TranscriptionState,
  type VideoSource,
} from "../services/meeting";

type SourceToggleProps = {
  id: string;
  icon: "mic" | "speaker" | "screen";
  label: string;
  description: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
};

function SourceToggle({
  id,
  icon,
  label,
  description,
  checked,
  onChange,
  disabled = false,
}: SourceToggleProps) {
  return (
    <label className="source-toggle" htmlFor={id}>
      <span className="source-toggle__icon">
        <Icon name={icon} size={22} />
      </span>
      <span className="source-toggle__copy">
        <strong>{label}</strong>
        <small>{description}</small>
      </span>
      <input
        id={id}
        className="toggle-input"
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span className="toggle-track" aria-hidden="true" />
    </label>
  );
}

export function NewRecordingPage({
  onOpenMeeting,
}: {
  onOpenMeeting: (id: string) => void;
}) {
  const [title, setTitle] = useState("Reunião de Produto");
  const [microphone, setMicrophone] = useState(true);
  const [systemAudio, setSystemAudio] = useState(true);
  const [video, setVideo] = useState(false);
  const [videoSourceId, setVideoSourceId] = useState("");
  const [videoSources, setVideoSources] = useState<VideoSource[]>([]);
  const [transcriptionThreads, setTranscriptionThreads] = useState(0);
  const [capture, setCapture] = useState<MeetingRecordingState | null>(null);
  const [transcription, setTranscription] = useState<TranscriptionState | null>(
    null,
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preferences, setPreferences] = useState<AppSettings | null>(null);
  const [settingsWarnings, setSettingsWarnings] = useState<string[]>([]);
  const active = capture?.active ?? false;

  useEffect(() => {
    if (!isTauri()) return;
    let mounted = true;
    void getSettings()
      .then((info) => {
        if (mounted) {
          setPreferences(info.settings);
          setSettingsWarnings(info.warnings);
        }
      })
      .catch((cause) => {
        if (mounted) setError(String(cause));
      });
    const refresh = () => {
      void getRecordingState()
        .then((state) => {
          if (mounted) setCapture(state);
        })
        .catch((cause: unknown) => {
          if (mounted) setError(String(cause));
        });
    };
    refresh();
    const subscriptions = Promise.all([
      listen("recording-status", refresh),
      listen("system-recording-status", refresh),
      listen("meeting-lifecycle-changed", refresh),
      listen("video-recording-status", refresh),
    ]).catch((cause: unknown) => {
      if (mounted) setError(String(cause));
      return [];
    });
    return () => {
      mounted = false;
      void subscriptions.then((unlisten) =>
        unlisten.forEach((remove) => remove()),
      );
    };
  }, []);

  async function refreshVideoSources() {
    try {
      const sources = await listVideoSources();
      setVideoSources(sources);
      setVideoSourceId((current) =>
        sources.some((source) => source.id === current) ? current : "",
      );
    } catch (cause) {
      setError(String(cause));
    }
  }

  function toggleVideo(enabled: boolean) {
    setVideo(enabled);
    if (enabled) void refreshVideoSources();
  }

  useEffect(() => {
    const id = capture?.meeting?.id;
    if (!isTauri() || !id || capture?.active) return;
    let mounted = true;
    const refresh = () => {
      void getTranscriptionState(id)
        .then((state) => {
          if (mounted) setTranscription(state);
        })
        .catch((cause: unknown) => {
          if (mounted) setError(String(cause));
        });
    };
    refresh();
    const timer = window.setInterval(refresh, 1000);
    return () => {
      mounted = false;
      window.clearInterval(timer);
    };
  }, [capture?.meeting?.id, capture?.active]);

  async function handleStart(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!title.trim() || busy) return;
    if (!isTauri()) {
      setError("Abra o aplicativo desktop para iniciar uma gravação.");
      return;
    }
    setBusy(true);
    setError(null);
    setTranscription(null);
    try {
      setCapture(
        await startMeeting({
          title: title.trim(),
          microphoneEnabled: microphone,
          systemAudioEnabled: systemAudio,
          videoEnabled: video,
          videoSourceId: video ? videoSourceId : undefined,
        }),
      );
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  async function handleStop() {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      setCapture(await stopMeeting(transcriptionThreads || undefined));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  async function handleCancelTranscription() {
    const id = capture?.meeting?.id;
    if (!id) return;
    try {
      setTranscription(await cancelTranscription(id));
    } catch (cause) {
      setError(String(cause));
    }
  }

  async function handleRetryTranscription() {
    const id = capture?.meeting?.id;
    if (!id) return;
    setError(null);
    try {
      setTranscription(
        await transcribeMeeting(id, transcriptionThreads || undefined),
      );
    } catch (cause) {
      setError(String(cause));
    }
  }

  return (
    <div className="page page--form">
      <header className="page-header page-header--stacked">
        <h1>Nova gravação</h1>
        <p>Escolha um nome e as fontes que deseja capturar.</p>
      </header>

      <div className="form-card">
        <div className="form-intro">
          <h2>Preparar reunião</h2>
          <p>Defina as opções antes de começar.</p>
        </div>

        <div className="inline-notice" role="note">
          <Icon name="info" size={19} />A gravação usa os dispositivos
          escolhidos nas configurações e salva os arquivos localmente. A
          transcrição será iniciada após a finalização da gravação, neste
          computador.
        </div>

        {error && (
          <div className="inline-notice" role="alert">
            {error}
          </div>
        )}
        {Array.from(
          new Set([...settingsWarnings, ...(capture?.warnings ?? [])]),
        ).map((message) => (
          <div className="inline-notice" role="status" key={message}>
            {message}
          </div>
        ))}
        {capture?.errors.map((message) => (
          <div className="inline-notice" role="alert" key={message}>
            {message}
          </div>
        ))}

        {active && capture ? (
          <div className="demo-active" role="status">
            <span className="demo-active__symbol" aria-hidden="true" />
            <div>
              <h3>Gravação em andamento</h3>
              <p>
                {capture.meeting?.title} · microfone:{" "}
                {capture.microphone?.status ?? "desativado"} · áudio do
                computador: {capture.system?.status ?? "desativado"}.
                {capture.video &&
                  ` Vídeo: ${capture.video.status} · ${capture.video.encoder}${capture.video.hardwareAccelerated ? " (hardware)" : ""}.`}
              </p>
            </div>
            <button
              className="button button--outline"
              type="button"
              disabled={busy}
              onClick={() => void handleStop()}
            >
              {busy ? "Finalizando…" : "Finalizar gravação"}
            </button>
          </div>
        ) : (
          <>
            {capture?.meeting && (
              <div className="inline-notice" role="status">
                {capture.meeting.title}: {capture.meeting.status} ·{" "}
                {capture.meeting.durationSeconds} s. Arquivos salvos localmente.
              </div>
            )}
            {capture?.meeting && (
              <button
                className="button button--outline"
                type="button"
                onClick={() => onOpenMeeting(capture.meeting!.id)}
              >
                Abrir reunião salva
              </button>
            )}
            {capture?.meeting && transcription && (
              <div
                className="inline-notice inline-notice--transcription"
                role="status"
              >
                <div>
                  Pós-processamento:{" "}
                  {transcription.status === "processing"
                    ? `${transcription.stage === "diarization" ? "identificando participantes" : transcription.stage === "source_transcription" ? "transcrevendo fontes" : "processando"} (${transcription.progress}%)`
                    : transcription.status === "completed"
                      ? "concluída e salva localmente"
                      : transcription.status === "cancelled"
                        ? "cancelada"
                        : transcription.status === "failed"
                          ? "falhou; a gravação foi preservada"
                          : "pendente"}
                  .{transcription.error && ` ${transcription.error}`}
                </div>
                {transcription.status === "processing" && (
                  <>
                    <progress
                      value={transcription.progress}
                      max={100}
                      aria-label="Progresso da transcrição"
                    />
                    <button
                      type="button"
                      className="button button--outline"
                      onClick={() => void handleCancelTranscription()}
                    >
                      Cancelar transcrição
                    </button>
                  </>
                )}
                {(transcription.status === "failed" ||
                  transcription.status === "cancelled") && (
                  <button
                    type="button"
                    className="button button--outline"
                    onClick={() => void handleRetryTranscription()}
                  >
                    Tentar transcrever novamente
                  </button>
                )}
              </div>
            )}
            <form onSubmit={handleStart}>
              <div className="field-group">
                <label htmlFor="meeting-title">Nome da reunião</label>
                <input
                  id="meeting-title"
                  type="text"
                  required
                  maxLength={100}
                  value={title}
                  onChange={(event) => setTitle(event.target.value)}
                  placeholder="Ex.: Reunião de Produto"
                />
              </div>

              <fieldset className="source-group">
                <legend>Áudio</legend>
                <SourceToggle
                  id="microphone"
                  icon="mic"
                  label="Microfone"
                  description="Sua voz durante a reunião"
                  checked={microphone}
                  onChange={setMicrophone}
                />
                <SourceToggle
                  id="system-audio"
                  icon="speaker"
                  label="Áudio do computador"
                  description="Som reproduzido pelo Windows"
                  checked={systemAudio}
                  onChange={setSystemAudio}
                />
              </fieldset>

              <div className="field-group">
                <label htmlFor="transcription-threads">
                  Threads para transcrição após a reunião
                </label>
                <select
                  id="transcription-threads"
                  value={transcriptionThreads}
                  onChange={(event) =>
                    setTranscriptionThreads(Number(event.target.value))
                  }
                >
                  <option value={0}>Automático (até 4)</option>
                  <option value={1}>1 thread</option>
                  <option value={2}>2 threads</option>
                  <option value={4}>Até 4 threads</option>
                </select>
              </div>

              <fieldset className="source-group">
                <legend>Vídeo</legend>
                <SourceToggle
                  id="screen-video"
                  icon="screen"
                  label="Gravar tela"
                  description={`Janela ou monitor · H.264 · ${preferences?.videoResolution ?? "720p"} · ${preferences?.videoFps ?? 15} FPS`}
                  checked={video}
                  onChange={toggleVideo}
                />
                {video && (
                  <div className="field-group">
                    <label htmlFor="video-source">
                      Janela ou monitor para gravar
                    </label>
                    <select
                      id="video-source"
                      required
                      value={videoSourceId}
                      onChange={(event) => setVideoSourceId(event.target.value)}
                    >
                      <option value="">Selecione uma fonte</option>
                      {videoSources.map((source) => (
                        <option key={source.id} value={source.id}>
                          {source.kind === "window" ? "Janela" : "Monitor"}:{" "}
                          {source.title}
                        </option>
                      ))}
                    </select>
                    <button
                      type="button"
                      className="button button--outline"
                      onClick={() => void refreshVideoSources()}
                    >
                      Atualizar fontes
                    </button>
                    <p>
                      A janela fonte deve permanecer aberta. Ao minimizar a
                      fonte, a imagem pode ficar congelada. Minimizar o Meeting
                      Recorder mantém a captura ativa.
                    </p>
                  </div>
                )}
              </fieldset>

              <div className="form-actions">
                <p>As gravações permanecem neste computador.</p>
                <button
                  className="button button--primary"
                  type="submit"
                  disabled={
                    busy ||
                    transcription?.status === "processing" ||
                    (!microphone && !systemAudio && !video) ||
                    (video && !videoSourceId)
                  }
                >
                  <span className="record-dot" aria-hidden="true" />
                  {busy ? "Iniciando…" : "Iniciar gravação"}
                </button>
              </div>
            </form>
          </>
        )}
      </div>
    </div>
  );
}
