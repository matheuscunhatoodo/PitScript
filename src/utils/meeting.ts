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
