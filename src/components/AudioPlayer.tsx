import { useEffect, useRef, useState, type SyntheticEvent } from "react";
import { clampSeek, formatDuration } from "../utils/meeting";
import { Icon } from "./Icon";

function playableDuration(media: HTMLMediaElement | null) {
  return media &&
    !media.error &&
    media.readyState >= 1 &&
    Number.isFinite(media.duration)
    ? media.duration
    : 0;
}

function mainMedia(
  audio: HTMLAudioElement | null,
  video: HTMLVideoElement | null,
) {
  return playableDuration(video) > playableDuration(audio)
    ? video
    : (audio ?? video);
}

function cancelledPlay(cause: unknown) {
  return cause instanceof DOMException && cause.name === "AbortError";
}

export function AudioPlayer({
  url,
  videoUrl = null,
  onTimeChange,
  seekRequest,
}: {
  url: string | null;
  videoUrl?: string | null;
  onTimeChange: (time: number) => void;
  seekRequest: { seconds: number; revision: number } | null;
}) {
  const audio = useRef<HTMLAudioElement>(null);
  const video = useRef<HTMLVideoElement>(null);
  const clock = useRef<HTMLMediaElement | null>(null);
  const transfer = useRef<{
    source: HTMLMediaElement;
    target: number;
    resume: boolean;
  } | null>(null);
  const player = useRef<HTMLDivElement>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [duration, setDuration] = useState(0);
  const [time, setTime] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [videoError, setVideoError] = useState<string | null>(null);
  const live = useRef(true);
  const appliedSeek = useRef(-1);
  const hasVideo = !!videoUrl;
  const label = hasVideo ? "gravação" : "áudio";
  useEffect(() => {
    const changed = () =>
      setFullscreen(document.fullscreenElement === player.current);
    document.addEventListener("fullscreenchange", changed);
    return () => document.removeEventListener("fullscreenchange", changed);
  }, []);
  function master() {
    return clock.current ?? mainMedia(audio.current, video.current);
  }

  useEffect(() => {
    live.current = true;
    clock.current = null;
    transfer.current = null;
    const elements = [audio.current, video.current];
    for (const [element, source] of [
      [audio.current, url],
      [video.current, videoUrl],
    ] as const) {
      if (element && source) {
        element.src = source;
        element.load();
      }
    }
    return () => {
      live.current = false;
      transfer.current = null;
      for (const element of elements) {
        if (element) {
          element.pause();
          element.removeAttribute("src");
          element.load();
        }
      }
    };
  }, [url, videoUrl]);

  function selectClock() {
    const previous = clock.current;
    const selected = mainMedia(audio.current, video.current);
    clock.current = selected;
    if (
      previous &&
      selected &&
      previous !== selected &&
      playableDuration(selected)
    ) {
      const pending = transfer.current;
      const resume =
        pending?.source === previous
          ? pending.resume
          : !previous.paused && !previous.ended;
      const target = clampSeek(
        pending?.source === previous ? pending.target : previous.currentTime,
        selected.duration,
      );
      previous.pause();
      selected.pause();
      selected.playbackRate = previous.playbackRate;
      if (Math.abs(selected.currentTime - target) > 0.05) {
        transfer.current = { source: selected, target, resume };
        selected.currentTime = target;
      } else if (resume && target < selected.duration) {
        transfer.current = null;
        void selected.play().catch((cause: unknown) => {
          if (live.current && !cancelledPlay(cause))
            setError(
              "Não foi possível continuar a reprodução. Tente novamente.",
            );
        });
      }
      setTime(target);
      onTimeChange(target);
      setPlaying(resume && target < selected.duration);
    }
    setDuration(
      Math.max(
        playableDuration(audio.current),
        playableDuration(video.current),
      ),
    );
    return selected;
  }

  function syncCompanion(source: HTMLMediaElement, forceSeek = false) {
    const screen = source === audio.current ? video.current : audio.current;
    if (transfer.current?.source === source) {
      screen?.pause();
      return;
    }
    if (
      !screen ||
      screen.error ||
      !Number.isFinite(screen.duration) ||
      !screen.duration
    )
      return;
    const target = clampSeek(source.currentTime, screen.duration);
    if (forceSeek || Math.abs(screen.currentTime - target) > 0.3)
      screen.currentTime = target;
    screen.playbackRate = source.playbackRate;
    if (
      source.paused ||
      source.ended ||
      source.readyState < 3 ||
      target >= screen.duration
    )
      screen.pause();
    else if (screen.paused)
      void screen.play().catch((cause: unknown) => {
        if (live.current && !cancelledPlay(cause) && !source.paused) {
          if (screen === video.current)
            setVideoError(
              "Não foi possível reproduzir o vídeo. O áudio continua disponível.",
            );
          else
            setError(
              "Não foi possível reproduzir o áudio. O vídeo continua disponível.",
            );
        }
      });
  }
  function seek(seconds: number) {
    const source = master();
    if (!source || !duration) return;
    const target = clampSeek(seconds, duration);
    source.currentTime = target;
    if (transfer.current?.source === source) transfer.current.target = target;
    syncCompanion(source, true);
    setTime(target);
    onTimeChange(target);
  }
  useEffect(() => {
    const source = clock.current ?? mainMedia(audio.current, video.current);
    if (
      !seekRequest ||
      !duration ||
      !source ||
      appliedSeek.current === seekRequest.revision
    )
      return;
    const target = clampSeek(seekRequest.seconds, duration);
    source.currentTime = target;
    if (transfer.current?.source === source) transfer.current.target = target;
    const screen = source === audio.current ? video.current : audio.current;
    if (screen && Number.isFinite(screen.duration) && screen.duration > 0) {
      screen.currentTime = clampSeek(target, screen.duration);
    }
    setTime(target);
    onTimeChange(target);
    appliedSeek.current = seekRequest.revision;
  }, [seekRequest, duration, onTimeChange, url]);
  async function toggle() {
    const source = master();
    if (!source) return;
    if (transfer.current?.source === source) {
      transfer.current.resume = !transfer.current.resume;
      setPlaying(transfer.current.resume);
      return;
    }
    if (!source.paused) {
      source.pause();
      syncCompanion(source);
      return;
    }
    setError(null);
    try {
      await source.play();
    } catch (cause) {
      if (live.current && !cancelledPlay(cause))
        setError(
          `Não foi possível reproduzir ${hasVideo ? "a gravação" : "o áudio"}. Tente novamente ou consulte a pasta da reunião.`,
        );
    }
  }
  function metadata() {
    const source = selectClock();
    if (source) syncCompanion(source, true);
  }
  function durationChanged() {
    // durationchange can precede loadedmetadata; switching the clock there can
    // lose its pending seek when the new element finishes initializing.
    setDuration(
      Math.max(
        playableDuration(audio.current),
        playableDuration(video.current),
      ),
    );
  }
  function update(event: SyntheticEvent<HTMLMediaElement>) {
    const source = event.currentTarget;
    if (source !== master()) return;
    if (transfer.current?.source === source) return;
    setTime(source.currentTime);
    onTimeChange(source.currentTime);
    syncCompanion(source);
  }
  function playback(event: SyntheticEvent<HTMLMediaElement>) {
    if (event.type === "playing" && event.currentTarget === video.current)
      setVideoError(null);
    if (event.currentTarget !== master()) return;
    if (transfer.current?.source === event.currentTarget) return;
    setPlaying(!event.currentTarget.paused && !event.currentTarget.ended);
    syncCompanion(event.currentTarget);
  }
  function failed(event: SyntheticEvent<HTMLMediaElement>) {
    if (event.currentTarget === video.current) {
      setVideoError(
        "Não foi possível abrir o vídeo. Verifique o MP4 na pasta da reunião.",
      );
    }
    if (event.currentTarget === audio.current || !url) {
      setError(
        `Não foi possível abrir ${hasVideo && !url ? "o vídeo" : "o áudio"}. Verifique o arquivo na pasta da reunião.`,
      );
    }
    const source = selectClock();
    setPlaying(
      transfer.current?.source === source
        ? transfer.current.resume
        : !!source && !source.paused && !source.ended && !source.error,
    );
    if (source) syncCompanion(source);
  }
  const events = {
    onLoadedMetadata: metadata,
    onDurationChange: durationChanged,
    onTimeUpdate: update,
    onPlay: playback,
    onPlaying: playback,
    onPause: playback,
    onEnded: playback,
    onError: failed,
    onSeeked: (event: SyntheticEvent<HTMLMediaElement>) => {
      const pending = transfer.current;
      if (!pending || pending.source !== event.currentTarget) return;
      if (Math.abs(pending.source.currentTime - pending.target) > 0.1) {
        pending.source.currentTime = pending.target;
        return;
      }
      transfer.current = null;
      if (pending.resume && pending.target < pending.source.duration) {
        void pending.source.play().catch((cause: unknown) => {
          if (live.current && !cancelledPlay(cause))
            setError(
              "Não foi possível continuar a reprodução. Tente novamente.",
            );
        });
      }
      syncCompanion(pending.source, true);
    },
    onSeeking: (event: SyntheticEvent<HTMLMediaElement>) => {
      if (event.currentTarget === master())
        syncCompanion(event.currentTarget, true);
    },
    onRateChange: (event: SyntheticEvent<HTMLMediaElement>) => {
      if (event.currentTarget === master()) syncCompanion(event.currentTarget);
    },
    onWaiting: (event: SyntheticEvent<HTMLMediaElement>) => {
      if (event.currentTarget === master()) {
        (event.currentTarget === audio.current
          ? video.current
          : audio.current
        )?.pause();
      }
    },
  };
  return (
    <div ref={player} className="meeting-player">
      {videoUrl && (
        <section
          className="meeting-video"
          aria-labelledby="meeting-video-title"
        >
          <div className="meeting-video__header">
            <h2 id="meeting-video-title">Vídeo da reunião</h2>
            <button
              className="button button--quiet"
              onClick={() => {
                const request = fullscreen
                  ? document.exitFullscreen()
                  : player.current?.requestFullscreen();
                void request?.catch(() => {
                  setVideoError(
                    "Não foi possível abrir a tela cheia. O vídeo continua disponível nesta página.",
                  );
                });
              }}
            >
              {fullscreen ? "Sair da tela cheia" : "Tela cheia"}
            </button>
          </div>
          <video
            ref={video}
            src={videoUrl}
            muted={!!url}
            playsInline
            preload="metadata"
            aria-label="Vídeo da reunião"
            {...events}
          />
          <p className="panel-help">
            Use os controles abaixo para reproduzir{" "}
            {url ? "vídeo e áudio juntos" : "o vídeo"}.
          </p>
          {videoError && (
            <p className="inline-notice" role="alert">
              {videoError}
            </p>
          )}
        </section>
      )}
      <section className="audio-player" aria-label="Player da reunião">
        {url && <audio ref={audio} src={url} preload="metadata" {...events} />}
        <button
          className="audio-play"
          aria-label={playing ? `Pausar ${label}` : `Reproduzir ${label}`}
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
          aria-label={hasVideo ? "Posição da gravação" : "Posição do áudio"}
          aria-valuetext={`${formatDuration(time)} de ${formatDuration(duration)}`}
          onChange={(event) => seek(Number(event.target.value))}
        />
        <time
          aria-label={hasVideo ? "Duração da gravação" : "Duração do áudio"}
        >
          {formatDuration(duration)}
        </time>
        <label className="sr-only" htmlFor="audio-speed">
          Velocidade de reprodução
        </label>
        <select
          className="audio-speed"
          id="audio-speed"
          defaultValue="1"
          disabled={!duration}
          onChange={(event) => {
            const source = master();
            if (source) {
              source.playbackRate = Number(event.target.value);
              syncCompanion(source);
            }
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
    </div>
  );
}
