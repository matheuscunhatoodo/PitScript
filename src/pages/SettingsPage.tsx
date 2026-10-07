import { useCallback, useEffect, useState, type FormEvent } from "react";
import { Icon } from "../components/Icon";
import {
  getSettings,
  saveSettings,
  listInputDevices,
  listOutputDevices,
  type AppSettings,
  type AudioDevice,
  type SettingsInfo,
} from "../services/settings";

function DeviceSelect({
  id,
  label,
  devices,
  value,
  onChange,
}: {
  id: string;
  label: string;
  devices: AudioDevice[];
  value: string | null;
  onChange: (value: string | null) => void;
}) {
  return (
    <div className="field-group">
      <label htmlFor={id}>{label}</label>
      <select
        id={id}
        value={value ?? ""}
        onChange={(event) => onChange(event.target.value || null)}
      >
        <option value="">Padrão do Windows</option>
        {value && !devices.some((device) => device.id === value) && (
          <option value={value}>Dispositivo salvo indisponível</option>
        )}
        {devices.map((device) => (
          <option key={device.id} value={device.id}>
            {device.name}
            {device.isDefault ? " (padrão atual)" : ""}
          </option>
        ))}
      </select>
      <small>
        Se o dispositivo salvo desaparecer, o backend usa o padrão do Windows ou
        outro dispositivo ativo.
      </small>
    </div>
  );
}

export function SettingsPage() {
  const [info, setInfo] = useState<SettingsInfo | null>(null);
  const [draft, setDraft] = useState<AppSettings | null>(null);
  const [inputs, setInputs] = useState<AudioDevice[]>([]);
  const [outputs, setOutputs] = useState<AudioDevice[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [deviceWarnings, setDeviceWarnings] = useState<string[]>([]);

  const load = useCallback(async (isMounted: () => boolean) => {
    setLoading(true);
    const [preferences, microphone, output] = await Promise.allSettled([
      getSettings(),
      listInputDevices(),
      listOutputDevices(),
    ]);
    if (!isMounted()) return;
    if (preferences.status === "fulfilled") {
      setInfo(preferences.value);
      setDraft((current) => current ?? preferences.value.settings);
      setError(null);
    } else setError(String(preferences.reason));
    const warnings: string[] = [];
    if (microphone.status === "fulfilled") setInputs(microphone.value);
    else
      warnings.push(
        "Não foi possível listar os microfones. A preferência salva foi mantida.",
      );
    if (output.status === "fulfilled") setOutputs(output.value);
    else
      warnings.push(
        "Não foi possível listar as saídas. A preferência salva foi mantida.",
      );
    setDeviceWarnings(warnings);
    setLoading(false);
  }, []);

  useEffect(() => {
    let mounted = true;
    void load(() => mounted).catch((cause) => {
      if (mounted) {
        setError(String(cause));
        setLoading(false);
      }
    });
    return () => {
      mounted = false;
    };
  }, [load]);

  function change<K extends keyof AppSettings>(key: K, value: AppSettings[K]) {
    setDraft((current) => (current ? { ...current, [key]: value } : current));
    setSaved(false);
  }

  async function handleSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!draft || saving || loading || info?.busy) return;
    setSaving(true);
    setError(null);
    setSaved(false);
    try {
      const persisted = await saveSettings(draft);
      setInfo(persisted);
      setDraft(persisted.settings);
      setSaved(true);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="page page--form">
      <header className="page-header page-header--stacked">
        <h1>Configurações</h1>
        <p>Preferências locais para novas gravações e transcrições.</p>
      </header>
      <form className="form-card settings-card" onSubmit={handleSave}>
        <div className="form-intro">
          <h2>Preferências</h2>
          <p>Salvas neste computador. Nenhum download ou serviço externo.</p>
        </div>
        {loading && <p role="status">Carregando preferências…</p>}
        {error && (
          <p className="inline-notice" role="alert">
            {error}
          </p>
        )}
        {[...(info?.warnings ?? []), ...deviceWarnings].map((warning) => (
          <p key={warning} className="inline-notice" role="status">
            {warning}
          </p>
        ))}
        {info?.busy && (
          <p className="inline-notice" role="status">
            Finalize a gravação e o processamento antes de salvar alterações.
          </p>
        )}
        {draft && (
          <fieldset
            className="settings-fields"
            disabled={loading || saving || info?.busy}
          >
            <div className="settings-section">
              <h3>Áudio</h3>
              <DeviceSelect
                id="default-microphone"
                label="Microfone padrão"
                devices={inputs}
                value={draft.microphoneDeviceId}
                onChange={(value) => change("microphoneDeviceId", value)}
              />
              <DeviceSelect
                id="default-output"
                label="Dispositivo de saída"
                devices={outputs}
                value={draft.outputDeviceId}
                onChange={(value) => change("outputDeviceId", value)}
              />
            </div>
            <div className="settings-section">
              <h3>Transcrição</h3>
              <div className="field-group">
                <label htmlFor="model">Modelo Whisper</label>
                <select
                  id="model"
                  value={draft.transcriptionModel}
                  onChange={(event) =>
                    change("transcriptionModel", event.target.value)
                  }
                >
                  {info?.models.map((model) => (
                    <option
                      key={model.id}
                      value={model.id}
                      disabled={
                        !model.available &&
                        model.id !== draft.transcriptionModel
                      }
                    >
                      {model.name}
                      {model.available ? "" : " — arquivo indisponível"}
                    </option>
                  ))}
                </select>
                <small>
                  Somente modelos multilíngues locais. Outros modelos são
                  habilitados quando seus arquivos estão instalados.
                </small>
              </div>
              <div className="field-group">
                <label htmlFor="language">Idioma</label>
                <select
                  id="language"
                  value={draft.language}
                  onChange={(event) => change("language", event.target.value)}
                >
                  {[
                    ["pt", "Português"],
                    ["en", "Inglês"],
                    ["es", "Espanhol"],
                    ["fr", "Francês"],
                    ["de", "Alemão"],
                    ["it", "Italiano"],
                    ["ja", "Japonês"],
                    ["zh", "Chinês"],
                    ["auto", "Detectar automaticamente"],
                  ].map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>
              </div>
              <div className="field-group">
                <label htmlFor="max-threads">
                  Quantidade máxima de threads
                </label>
                <input
                  id="max-threads"
                  type="number"
                  min={1}
                  max={16}
                  step={1}
                  required
                  value={draft.maxThreads}
                  onChange={(event) =>
                    change("maxThreads", Number(event.target.value))
                  }
                />
                <small>
                  De 1 a 16, limitado também pelos processadores disponíveis. IA
                  é executada somente depois da reunião.
                </small>
              </div>
            </div>
            <div className="settings-section">
              <h3>Vídeo</h3>
              <div className="field-group">
                <label htmlFor="video-resolution">Resolução</label>
                <select
                  id="video-resolution"
                  value={draft.videoResolution}
                  onChange={(event) =>
                    change("videoResolution", event.target.value)
                  }
                >
                  <option value="480p">480p</option>
                  <option value="720p">720p (recomendado)</option>
                  <option value="1080p">1080p</option>
                </select>
              </div>
              <div className="field-group">
                <label htmlFor="video-fps">FPS</label>
                <select
                  id="video-fps"
                  value={draft.videoFps}
                  onChange={(event) =>
                    change("videoFps", Number(event.target.value))
                  }
                >
                  {[5, 10, 15, 30].map((fps) => (
                    <option key={fps} value={fps}>
                      {fps}
                      {fps === 15 ? " (recomendado)" : ""}
                    </option>
                  ))}
                </select>
                <small>
                  Aplicado quando você habilitar vídeo em uma nova gravação.
                  Qualidades maiores aumentam o consumo.
                </small>
              </div>
            </div>
            <div className="settings-section">
              <h3>Armazenamento</h3>
              <div className="field-group">
                <label htmlFor="recordings-path">Diretório de gravações</label>
                <input
                  id="recordings-path"
                  type="text"
                  value={draft.recordingsDirectory ?? ""}
                  onChange={(event) =>
                    change("recordingsDirectory", event.target.value || null)
                  }
                  placeholder="Diretório padrão do aplicativo"
                />
                <small>
                  Deixe vazio para usar o padrão. Use uma pasta local gravável.
                  Reuniões existentes permanecem nas pastas originais.
                </small>
                <small>
                  Diretório em uso: {info?.effectiveRecordingsDirectory}
                </small>
              </div>
            </div>
          </fieldset>
        )}
        <div className="form-actions">
          {saved ? (
            <p className="save-feedback" role="status">
              <Icon name="check" size={17} />
              Configurações salvas no computador.
            </p>
          ) : (
            <p>Alterações serão aplicadas às próximas operações.</p>
          )}
          <button
            className="button button--secondary"
            type="button"
            disabled={loading || saving}
            onClick={() =>
              void load(() => true).catch((cause) => {
                setError(String(cause));
                setLoading(false);
              })
            }
          >
            Atualizar dispositivos e estado
          </button>
          <button
            className="button button--primary"
            type="submit"
            disabled={!draft || loading || saving || info?.busy}
          >
            {saving ? "Salvando…" : "Salvar configurações"}
          </button>
        </div>
      </form>
    </div>
  );
}
