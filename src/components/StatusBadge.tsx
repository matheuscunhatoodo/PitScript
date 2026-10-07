import type { MeetingStatus } from "../types/meeting";

const labels: Record<MeetingStatus, string> = {
  created: "Criada",
  recording: "Gravando",
  completed: "Concluída",
  processing: "Processando",
  failed_partial: "Falha parcial",
  failed: "Falhou",
};

export function StatusBadge({ status }: { status: MeetingStatus }) {
  return <span className={`status status--${status}`}>{labels[status]}</span>;
}
