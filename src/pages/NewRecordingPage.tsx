import { useEffect, useState, type FormEvent } from "react";
import { Icon } from "../components/Icon";
import {
  getSettings,
  listInputDevices,
  listOutputDevices,
  type AppSettings,
  type AudioDevice,
} from "../services/settings";
import {
  cancelTranscription,
  listVideoSources,
  transcribeMeeting,
  type VideoSource,
} from "../services/meeting";
import type { RecordingController } from "../hooks/useRecording";
import { formatDuration, sourceStatusLabel } from "../utils/meeting";

function SourceToggle({
  id,
  icon,
  label,
  description,
  checked,
  onChange,
}: {
  id: string;
  icon: "mic" | "speaker" | "screen";
  label: string;
  description: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <label className="source-toggle" htmlFor={id}>
      <Icon name={icon} size={24} />
      <span className="source-toggle__copy">
        <strong>{label}</strong>
        <small>{description}</small>
      </span>
      <input
        id={id}
        className="toggle-input"
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="toggle-track" aria-hidden="true" />
    </label>
  );
}
function DeviceSelect({
  id,
  label,
  devices,
  value,
  configuredId,
  onChange,
}: {
  id: string;
  label: string;
  devices: AudioDevice[];
  value: string;
  configuredId: string | null;
  onChange: (value: string) => void;
}) {
  return (
    <div className="field-group device-field">
      <label htmlFor={id}>{label}</label>
      <select id={id} value={value} onChange={(e) => onChange(e.target.value)}>
        <option value="">
          {configuredId
            ? devices.find((device) => device.id === configuredId)
              ? `Preferência salva: ${devices.find((device) => device.id === configuredId)!.name} · com fallback`
              : "Preferência salva indisponível · usar fallback"
            : "Automático · padrão do Windows"}
        </option>
        {value && !devices.some((d) => d.id === value) && (
          <option value={value}>
            Dispositivo selecionado indisponível · escolha outro
          </option>
        )}
        {devices.map((d) => (
          <option key={d.id} value={d.id}>
            {d.name}
            {d.isDefault ? " (padrão)" : ""}
          </option>
        ))}
      </select>
    </div>
  );
}
export function NewRecordingPage({
  onOpenMeeting,
  recording,
}: {
  onOpenMeeting: (id: string) => void;
  recording: RecordingController;
}) {
  const [title, setTitle] = useState(
    () =>
      `Reunião de ${new Intl.DateTimeFormat("pt-BR", { day: "2-digit", month: "2-digit" }).format(new Date())}`,
  );
  const [microphone, setMicrophone] = useState(true);
  const [systemAudio, setSystemAudio] = useState(true);
  const [video, setVideo] = useState(false);
  const [microphoneId, setMicrophoneId] = useState("");
  const [outputId, setOutputId] = useState("");
  const [inputs, setInputs] = useState<AudioDevice[]>([]);
  const [outputs, setOutputs] = useState<AudioDevice[]>([]);
  const [videoSourceId, setVideoSourceId] = useState("");
  const [videoSources, setVideoSources] = useState<VideoSource[]>([]);
  const [preferences, setPreferences] = useState<AppSettings | null>(null);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [localError, setLocalError] = useState<string | null>(null);
  const [actionBusy, setActionBusy] = useState(false);
  const { capture, transcription, busy, ready, error } = recording;
  const active = capture?.active ?? false;
  useEffect(() => {
    let live = true;
    void Promise.allSettled([
      Promise.resolve().then(getSettings),
      Promise.resolve().then(listInputDevices),
      Promise.resolve().then(listOutputDevices),
    ]).then(([saved, mic, sys]) => {
      if (!live) return;
      const messages: string[] = [];
      if (saved.status === "fulfilled") {
        setPreferences(saved.value.settings);
        messages.push(...saved.value.warnings);
      } else
        messages.push(
          "Não foi possível consultar as preferências. O backend usará os padrões seguros.",
        );
      if (mic.status === "fulfilled") setInputs(mic.value);
      else
        messages.push(
          "Não foi possível listar microfones. Será usado o dispositivo padrão disponível.",
        );
      if (sys.status === "fulfilled") setOutputs(sys.value);
      else
        messages.push(
          "Não foi possível listar as saídas. Será usado o dispositivo padrão disponível.",
        );
      setWarnings(messages);
    });
    return () => {
      live = false;
    };
  }, []);
  async function refreshVideo() {
    try {
      const sources = await listVideoSources();
      setVideoSources(sources);
      setVideoSourceId((id) => (sources.some((s) => s.id === id) ? id : ""));
    } catch (cause) {
      setLocalError(
        `Não foi possível listar janelas e monitores. ${String(cause)}`,
      );
    }
  }
  function handleStart(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setLocalError(null);
    if (title.trim())
      void recording.start({
        title: title.trim(),
        microphoneEnabled: microphone,
        systemAudioEnabled: systemAudio,
        videoEnabled: video,
        microphoneDeviceId: microphoneId || undefined,
        outputDeviceId: outputId || undefined,
        videoSourceId: video ? videoSourceId : undefined,
      });
  }
  async function finalize() {
    const state = await recording.stop();
    if (state?.meeting && !state.active) onOpenMeeting(state.meeting.id);
  }
  async function processAction(cancel: boolean) {
    if (!capture?.meeting || actionBusy) return;
    setActionBusy(true);
    setLocalError(null);
    try {
      if (cancel) await cancelTranscription(capture.meeting.id);
      else await transcribeMeeting(capture.meeting.id);
      await recording.refresh();
    } catch (cause) {
      setLocalError(String(cause));
    } finally {
      setActionBusy(false);
    }
  }
  const sourceRows = [
    {
      label: "Microfone",
      description: "Sua voz durante a reunião",
      icon: "mic" as const,
      state: capture?.microphone,
    },
    {
      label: "Áudio do computador",
      description: "Som reproduzido pelo Windows",
      icon: "screen" as const,
      state: capture?.system,
    },
    {
      label: capture?.video ? "Vídeo" : "Vídeo desativado",
      description: capture?.video ? "Janela ou monitor selecionado" : "",
      icon: "screen" as const,
      state: capture?.video,
    },
  ];
  return (
    <div className={`page ${active ? "page--recording" : "page--form"}`}>
      <header className="page-header page-header--stacked">
        <h1>{active ? "Gravação em andamento" : "Nova gravação"}</h1>
        <p>
          {active
            ? capture?.meeting?.title
            : "Escolha um nome e as fontes que deseja capturar."}
        </p>
      </header>
      {(error || localError) && (
        <div className="inline-notice" role="alert">
          {error || localError}
          <button
            className="text-button"
            onClick={() => void recording.refresh()}
          >
            Atualizar estado
          </button>
        </div>
      )}
      {[...new Set([...warnings, ...(capture?.warnings ?? [])])].map(
        (message) => (
          <p
            className="inline-notice inline-notice--warning"
            role="status"
            key={message}
          >
            {message}
          </p>
        ),
      )}
      {!!capture?.errors.length && (
        <div className="inline-notice inline-notice--warning" role="alert">
          <strong>
            Uma fonte foi interrompida. Os arquivos válidos serão preservados.
          </strong>
          <details>
            <summary>Ver detalhes</summary>
            {capture.errors.map((message) => (
              <p key={message}>{message}</p>
            ))}
          </details>
        </div>
      )}
      {active ? (
        <section
          className="recording-console"
          aria-label="Captura em andamento"
        >
          <div className="recording-heading">
            <span className="record-dot" />
            <strong>{busy ? "Finalizando…" : "Gravando"}</strong>
          </div>
          <p className="recording-subtitle">
            {[
              capture?.microphone && "Microfone",
              capture?.system && "áudio do computador",
              capture?.video && "vídeo",
            ]
              .filter(Boolean)
              .join(" e ")}
          </p>
          <div className="recording-clock" aria-label="Tempo de gravação">
            {formatDuration(
              capture?.elapsedSeconds ?? capture?.meeting?.durationSeconds ?? 0,
            )}
          </div>
          <p className="clock-caption">Tempo de gravação</p>
          <div className="capture-sources">
            {sourceRows.map((source) => (
              <div
                className={`capture-source${source.state ? "" : " capture-source--disabled"}`}
                key={source.label}
              >
                <Icon name={source.icon} size={31} />
                <span>
                  <strong>{source.label}</strong>
                  <small>{source.description}</small>
                </span>
                <span
                  className={`source-state source-state--${source.state?.status ?? "idle"}`}
                >
                  <span className="status-dot" />
                  {sourceStatusLabel(source.state?.status)}
                </span>
              </div>
            ))}
          </div>
          <button
            className="button button--danger recording-stop"
            disabled={busy}
            onClick={() => void finalize()}
          >
            <Icon name="stop" size={19} />
            {busy ? "Finalizando…" : "Finalizar gravação"}
          </button>
          <p className="panel-help">
            {capture?.microphone || capture?.system
              ? "O áudio será salvo e a transcrição começará em seguida."
              : "O vídeo será salvo na pasta da reunião."}
          </p>
          <p className="minimize-note">
            <Icon name="minimize" size={20} />
            Pode minimizar. A gravação continua.
          </p>
        </section>
      ) : (
        <div className="form-card">
          {capture?.meeting && (
            <section className="saved-session">
              <strong>{capture.meeting.title}</strong>
              <span>
                Gravação finalizada ·{" "}
                {formatDuration(capture.meeting.durationSeconds)}
              </span>
              <button
                className="text-button"
                onClick={() => onOpenMeeting(capture.meeting!.id)}
              >
                Abrir reunião salva
              </button>
              {transcription && (
                <div className="postprocess-state" role="status">
                  <p>
                    {transcription.status === "processing"
                      ? `${transcription.stage === "diarization" ? "Identificando participantes" : transcription.stage === "source_transcription" ? "Transcrevendo fontes" : "Transcrevendo"} · ${transcription.progress}%`
                      : transcription.status === "completed"
                        ? "Transcrição pronta"
                        : transcription.status === "failed"
                          ? "A transcrição falhou. O áudio foi preservado."
                          : transcription.status === "cancelled"
                            ? "Transcrição cancelada. O áudio foi preservado."
                            : "Áudio salvo"}
                  </p>
                  {transcription.status === "processing" && (
                    <>
                      <progress
                        value={transcription.progress}
                        max={100}
                        aria-label="Progresso da transcrição"
                      />
                      <button
                        className="text-button"
                        disabled={actionBusy}
                        onClick={() => void processAction(true)}
                      >
                        Cancelar transcrição
                      </button>
                    </>
                  )}
                  {["failed", "cancelled"].includes(transcription.status) && (
                    <button
                      className="text-button"
                      disabled={actionBusy}
                      onClick={() => void processAction(false)}
                    >
                      Tentar novamente
                    </button>
                  )}
                </div>
              )}
            </section>
          )}
          <form onSubmit={handleStart}>
            <div className="field-group">
              <label htmlFor="meeting-title">Nome da reunião</label>
              <input
                id="meeting-title"
                required
                maxLength={100}
                value={title}
                onChange={(e) => setTitle(e.target.value)}
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
              {microphone && (
                <DeviceSelect
                  id="microphone-device"
                  label="Microfone selecionado"
                  devices={inputs}
                  value={microphoneId}
                  configuredId={preferences?.microphoneDeviceId ?? null}
                  onChange={setMicrophoneId}
                />
              )}
              <SourceToggle
                id="system-audio"
                icon="speaker"
                label="Áudio do computador"
                description="Som reproduzido pelo Windows"
                checked={systemAudio}
                onChange={setSystemAudio}
              />
              {systemAudio && (
                <DeviceSelect
                  id="output-device"
                  label="Saída de áudio selecionada"
                  devices={outputs}
                  value={outputId}
                  configuredId={preferences?.outputDeviceId ?? null}
                  onChange={setOutputId}
                />
              )}
            </fieldset>
            <fieldset className="source-group">
              <legend>Vídeo opcional</legend>
              <SourceToggle
                id="screen-video"
                icon="screen"
                label="Gravar tela"
                description={`Janela ou monitor · ${preferences?.videoResolution ?? "720p"} · ${preferences?.videoFps ?? 15} FPS`}
                checked={video}
                onChange={(value) => {
                  setVideo(value);
                  if (value) void refreshVideo();
                }}
              />
              {video && (
                <div className="field-group">
                  <label htmlFor="video-source">Janela ou monitor</label>
                  <select
                    id="video-source"
                    value={videoSourceId}
                    required
                    onChange={(e) => setVideoSourceId(e.target.value)}
                  >
                    <option value="">Selecione uma fonte</option>
                    {videoSources.map((s) => (
                      <option key={s.id} value={s.id}>
                        {s.kind === "window" ? "Janela" : "Monitor"}: {s.title}
                      </option>
                    ))}
                  </select>
                  <button
                    className="text-button"
                    type="button"
                    onClick={() => void refreshVideo()}
                  >
                    Atualizar fontes
                  </button>
                  <small>
                    A janela selecionada deve permanecer aberta. Minimizar essa
                    janela pode congelar a imagem.
                  </small>
                </div>
              )}
            </fieldset>
            <div className="form-actions">
              <p>Transcrição local após finalizar.</p>
              <button
                className="button button--primary"
                type="submit"
                disabled={
                  !ready ||
                  busy ||
                  transcription?.status === "processing" ||
                  (!microphone && !systemAudio && !video) ||
                  (video && !videoSourceId)
                }
              >
                <Icon name="play" size={18} />
                {busy
                  ? "Iniciando…"
                  : !ready
                    ? "Consultando estado…"
                    : "Iniciar gravação"}
              </button>
            </div>
          </form>
        </div>
      )}
    </div>
  );
}
