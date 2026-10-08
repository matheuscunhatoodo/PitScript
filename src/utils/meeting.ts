export function findTranscriptMatches(
  text: string,
  query: string,
): { start: number; end: number }[] {
  const term = query.trim();
  if (!term) return [];
  const escaped = term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return Array.from(text.matchAll(new RegExp(escaped, "giu")), (match) => ({
    start: match.index,
    end: match.index + match[0].length,
  }));
}
export function formatDuration(seconds: number): string {
  const total = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60)
    .toString()
    .padStart(2, "0");
  const remaining = (total % 60).toString().padStart(2, "0");
  return hours ? `${hours}:${minutes}:${remaining}` : `${minutes}:${remaining}`;
}
export function formatMeetingDate(timestamp: string): {
  date: string;
  time: string;
} {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime()))
    return { date: "Data indisponível", time: "" };
  return {
    date: new Intl.DateTimeFormat("pt-BR", { dateStyle: "short" }).format(date),
    time: new Intl.DateTimeFormat("pt-BR", {
      hour: "2-digit",
      minute: "2-digit",
    }).format(date),
  };
}

export function formatSegmentTranscript(
  segments: { startMs: number; diarizationLabel: string; text: string }[],
): string {
  return segments
    .map((segment) => {
      const total = Math.max(0, Math.floor(segment.startMs / 1000));
      const timestamp = [
        Math.floor(total / 3600),
        Math.floor((total % 3600) / 60),
        total % 60,
      ]
        .map((value) => String(value).padStart(2, "0"))
        .join(":");
      return `${timestamp}\n${segment.diarizationLabel}:\n${segment.text.trim()}`;
    })
    .join("\n\n");
}

export function groupMeetingsByDay<T extends { startedAt: string }>(
  meetings: T[],
  now = new Date(),
) {
  const dayKey = (date: Date) =>
    `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
  const yesterday = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate() - 1,
  );
  const groups = new Map<
    string,
    { key: string; label: string; meetings: T[] }
  >();
  const sorted = [...meetings].sort(
    (a, b) => (Date.parse(b.startedAt) || 0) - (Date.parse(a.startedAt) || 0),
  );
  for (const meeting of sorted) {
    const date = new Date(meeting.startedAt);
    const valid = !Number.isNaN(date.getTime());
    const key = valid ? dayKey(date) : "unknown";
    if (!groups.has(key)) {
      const label = valid
        ? new Intl.DateTimeFormat("pt-BR", {
            day: "numeric",
            month: "long",
            ...(date.getFullYear() !== now.getFullYear()
              ? { year: "numeric" as const }
              : {}),
          }).format(date)
        : "Data indisponível";
      groups.set(key, {
        key,
        label:
          key === dayKey(now)
            ? `Hoje · ${label}`
            : key === dayKey(yesterday)
              ? `Ontem · ${label}`
              : label,
        meetings: [],
      });
    }
    groups.get(key)!.meetings.push(meeting);
  }
  return [...groups.values()];
}

export function meetingProgressLabel(meeting: {
  status: string;
  transcriptionStatus: string;
  transcription?: string | null;
}) {
  if (meeting.status === "recording") return "Gravando";
  if (meeting.status === "created") return "Preparando";
  if (meeting.status === "failed") return "Falha na gravação";
  if (meeting.status === "failed_partial")
    return meeting.transcriptionStatus === "processing"
      ? "Falha parcial · transcrevendo"
      : "Áudio salvo · falha parcial";
  if (meeting.transcriptionStatus === "processing") return "Transcrevendo";
  if (meeting.transcriptionStatus === "failed")
    return "Transcrição falhou · áudio salvo";
  if (meeting.transcriptionStatus === "cancelled")
    return "Transcrição cancelada";
  if (meeting.transcriptionStatus === "completed")
    return meeting.transcription?.trim() ? "Pronta" : "Sem texto reconhecido";
  return meeting.status === "processing" ? "Preparando áudio" : "Áudio salvo";
}

export function sourceStatusLabel(status?: string) {
  const labels: Record<string, string> = {
    recording: "Capturando",
    starting: "Iniciando",
    completed: "Salvo",
    failed: "Falhou",
    failed_partial: "Interrompida · áudio preservado",
    idle: "Desativado",
  };
  return status ? (labels[status] ?? "Estado indisponível") : "Desativado";
}

export function clampSeek(seconds: number, duration: number) {
  if (!Number.isFinite(seconds) || !Number.isFinite(duration) || duration <= 0)
    return 0;
  return Math.max(0, Math.min(seconds, duration));
}

export function buildTranscriptBlocks(
  segments: {
    id: string;
    startMs: number;
    endMs: number;
    diarizationLabel: string;
    text: string;
  }[],
) {
  let offset = 0;
  return segments.map((segment) => {
    const time = formatDuration(segment.startMs / 1000);
    const text = segment.text.trim();
    const searchText = `${time}\n${segment.diarizationLabel}:\n${text}`;
    const block = {
      ...segment,
      text,
      time,
      searchText,
      offset,
      labelOffset: offset + time.length + 1,
      bodyOffset: offset + time.length + segment.diarizationLabel.length + 3,
    };
    offset += searchText.length + 2;
    return block;
  });
}

export function activeSegmentIndices(
  segments: { startMs: number; endMs: number }[],
  seconds: number,
) {
  const ms = seconds * 1000;
  return segments.flatMap((segment, index) =>
    ms >= segment.startMs && ms < segment.endMs ? [index] : [],
  );
}
