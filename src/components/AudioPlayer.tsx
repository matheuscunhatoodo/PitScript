import { useEffect, useRef, useState } from "react";
import { clampSeek, formatDuration } from "../utils/meeting";
import { Icon } from "./Icon";

export function AudioPlayer({
  url,
  onTimeChange,
  seekRequest,
}: {
  url: string;
  onTimeChange: (time: number) => void;
  seekRequest: { seconds: number; revision: number } | null;
}) {
  const media = useRef<HTMLAudioElement>(null);
  const [playing, setPlaying] = useState(false);
  const [duration, setDuration] = useState(0);
  const [time, setTime] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const live = useRef(true);
  const appliedSeek = useRef(-1);
  useEffect(() => {
    live.current = true;
    const audio = media.current;
    if (audio) {
      audio.src = url;
      audio.load();
    }
    return () => {
      live.current = false;
      if (audio) {
        audio.pause();
        audio.removeAttribute("src");
        audio.load();
      }
    };
  }, [url]);
  function seek(seconds: number) {
    if (!media.current || !duration) return;
    const target = clampSeek(seconds, duration);
    media.current.currentTime = target;
    setTime(target);
    onTimeChange(target);
  }
  useEffect(() => {
    if (
      !seekRequest ||
      !duration ||
      !media.current ||
      appliedSeek.current === seekRequest.revision
    )
      return;
    const target = clampSeek(seekRequest.seconds, duration);
    media.current.currentTime = target;
    setTime(target);
    onTimeChange(target);
    appliedSeek.current = seekRequest.revision;
  }, [seekRequest, duration, onTimeChange]);
  async function toggle() {
    const audio = media.current;
    if (!audio) return;
    if (!audio.paused) {
      audio.pause();
      return;
    }
    setError(null);
    try {
      await audio.play();
    } catch {
      if (live.current)
        setError(
          "Não foi possível reproduzir o áudio. Tente novamente ou consulte a pasta da reunião.",
        );
    }
  }
  return (
    <section className="audio-player" aria-label="Player da reunião">
      <audio
        ref={media}
        src={url}
        preload="metadata"
        onLoadedMetadata={(event) => {
          const value = event.currentTarget.duration;
          setDuration(Number.isFinite(value) ? value : 0);
        }}
        onDurationChange={(event) => {
          const value = event.currentTarget.duration;
          if (Number.isFinite(value)) setDuration(value);
        }}
        onTimeUpdate={(event) => {
          const current = event.currentTarget.currentTime;
          setTime(current);
          onTimeChange(current);
        }}
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onEnded={() => setPlaying(false)}
        onError={() => {
          setPlaying(false);
          setDuration(0);
          setError(
            "Não foi possível abrir o áudio. Verifique o arquivo na pasta da reunião.",
          );
        }}
      />
      <button
        className="audio-play"
        aria-label={playing ? "Pausar áudio" : "Reproduzir áudio"}
        disabled={!duration}
        onClick={() => void toggle()}
      >
        <Icon name={playing ? "pause" : "play"} size={26} />
      </button>
      <button
        className="icon-button"
        aria-label="Voltar 10 segundos"
        disabled={!duration}
        onClick={() => seek(time - 10)}
      >
        <Icon name="rewind" size={26} />
      </button>
      <button
        className="icon-button"
        aria-label="Avançar 10 segundos"
        disabled={!duration}
        onClick={() => seek(time + 10)}
      >
        <Icon name="forward" size={26} />
      </button>
      <time aria-label="Tempo atual">{formatDuration(time)}</time>
      <input
        className="audio-seek"
        type="range"
        min={0}
        max={duration || 1}
        step={0.1}
        value={Math.min(time, duration || 0)}
        disabled={!duration}
        aria-label="Posição do áudio"
        aria-valuetext={`${formatDuration(time)} de ${formatDuration(duration)}`}
        onChange={(event) => seek(Number(event.target.value))}
      />
      <time aria-label="Duração do áudio">{formatDuration(duration)}</time>
      <label className="sr-only" htmlFor="audio-speed">
        Velocidade de reprodução
      </label>
      <select
        className="audio-speed"
        id="audio-speed"
        defaultValue="1"
        disabled={!duration}
        onChange={(event) => {
          if (media.current)
            media.current.playbackRate = Number(event.target.value);
        }}
      >
        {[0.75, 1, 1.25, 1.5, 2].map((speed) => (
          <option key={speed} value={speed}>
            {speed}×
          </option>
        ))}
      </select>
      {error && (
        <p role="alert" className="audio-player__error">
          {error}
        </p>
      )}
    </section>
  );
}
