import assert from "node:assert/strict";
import { test } from "node:test";
import {
  findTranscriptMatches,
  formatDuration,
  formatMeetingDate,
  formatSegmentTranscript,
  groupMeetingsByDay,
  meetingProgressLabel,
  sourceStatusLabel,
  clampSeek,
  buildTranscriptBlocks,
  activeSegmentIndices,
} from "../src/utils/meeting.ts";

test("search is literal, case insensitive and preserves original offsets", () => {
  const text = "Olá [ação]. Outra AÇÃO. [ação]";
  assert.deepEqual(
    findTranscriptMatches(text, "[ação]").map(({ start, end }) =>
      text.slice(start, end),
    ),
    ["[ação]", "[ação]"],
  );
  assert.equal(findTranscriptMatches(text, "ação").length, 3);
  assert.deepEqual(findTranscriptMatches(text, "  "), []);
  assert.deepEqual(findTranscriptMatches(text, "inexistente"), []);
  assert.equal(findTranscriptMatches("aaa", "aa").length, 1);
  assert.deepEqual(findTranscriptMatches("😀 teste TESTE", "teste"), [
    { start: 3, end: 8 },
    { start: 9, end: 14 },
  ]);
});

test("library groups by local calendar day, sorts newest first and retains invalid dates", () => {
  const now = new Date(2026, 9, 8, 12);
  const rows = [
    { id: "old", startedAt: new Date(2026, 9, 7, 23, 59).toISOString() },
    { id: "early", startedAt: new Date(2026, 9, 8, 0, 1).toISOString() },
    { id: "bad", startedAt: "invalid" },
    { id: "late", startedAt: new Date(2026, 9, 8, 11).toISOString() },
  ];
  const groups = groupMeetingsByDay(rows, now);
  assert.match(groups[0].label, /^Hoje/);
  assert.match(groups[1].label, /^Ontem/);
  assert.deepEqual(
    groups[0].meetings.map((m) => m.id),
    ["late", "early"],
  );
  assert.equal(groups[2].label, "Data indisponível");
  assert.equal(rows[0].id, "old", "does not mutate repository data");
});

test("labels distinguish preserved audio, failed sources and incomplete transcription", () => {
  assert.equal(
    meetingProgressLabel({
      status: "completed",
      transcriptionStatus: "pending",
    }),
    "Áudio salvo",
  );
  assert.equal(
    meetingProgressLabel({
      status: "completed",
      transcriptionStatus: "cancelled",
    }),
    "Transcrição cancelada",
  );
  assert.equal(
    meetingProgressLabel({
      status: "failed_partial",
      transcriptionStatus: "processing",
    }),
    "Falha parcial · transcrevendo",
  );
  assert.equal(
    meetingProgressLabel({
      status: "completed",
      transcriptionStatus: "completed",
      transcription: "Olá",
    }),
    "Pronta",
  );
  assert.equal(
    meetingProgressLabel({
      status: "completed",
      transcriptionStatus: "completed",
      transcription: "",
    }),
    "Sem texto reconhecido",
  );
  assert.equal(
    sourceStatusLabel("failed_partial"),
    "Interrompida · áudio preservado",
  );
  assert.equal(sourceStatusLabel("recording"), "Capturando");
  assert.equal(sourceStatusLabel(undefined), "Desativado");
});

test("seek clamps invalid metadata, negative positions and end of audio", () => {
  assert.equal(clampSeek(19, 2538), 19);
  assert.equal(clampSeek(-10, 100), 0);
  assert.equal(clampSeek(200, 100), 100);
  assert.equal(clampSeek(19, NaN), 0);
  assert.equal(clampSeek(Infinity, 100), 0);
});

test("structured search includes timestamps and labels, preserving Portuguese and offsets", () => {
  const blocks = buildTranscriptBlocks([
    {
      id: "a",
      startMs: 3000,
      endMs: 12000,
      diarizationLabel: "Você",
      text: " Olá, projeto. ",
    },
    {
      id: "b",
      startMs: 12000,
      endMs: 19000,
      diarizationLabel: "Participante 1",
      text: "AÇÃO [projeto]",
    },
  ]);
  const text = blocks.map((b) => b.searchText).join("\n\n");
  assert.equal(blocks[0].time, "00:03");
  assert.equal(
    text.slice(
      blocks[1].bodyOffset,
      blocks[1].bodyOffset + blocks[1].text.length,
    ),
    "AÇÃO [projeto]",
  );
  assert.equal(findTranscriptMatches(text, "projeto").length, 2);
  assert.equal(findTranscriptMatches(text, "Você").length, 1);
  assert.equal(findTranscriptMatches(text, "00:12").length, 1);
  assert.deepEqual(buildTranscriptBlocks([]), []);
});

test("playback marks both overlapping segments without inventing a single speaker", () => {
  const segments = [
    { startMs: 3000, endMs: 12000 },
    { startMs: 10000, endMs: 19000 },
  ];
  assert.deepEqual(activeSegmentIndices(segments, 2), []);
  assert.deepEqual(activeSegmentIndices(segments, 10), [0, 1]);
  assert.deepEqual(activeSegmentIndices(segments, 12), [1]);
  assert.deepEqual(activeSegmentIndices(segments, 19), []);
});

test("formats duration without wrapping after one hour", () => {
  assert.equal(formatDuration(0), "00:00");
  assert.equal(formatDuration(65), "01:05");
  assert.equal(formatDuration(3661), "1:01:01");
  assert.equal(formatDuration(-1), "00:00");
});

test("formats persisted dates and handles invalid timestamps", () => {
  assert.equal(formatMeetingDate("invalid").date, "Data indisponível");
  assert.equal(formatMeetingDate("2026-10-01T12:00:00Z").date, "01/10/2026");
  assert.match(formatMeetingDate("2026-10-01T12:00:00Z").time, /^\d{2}:\d{2}$/);
});

test("structured text retains timestamps and neutral labels for copy, export and search", () => {
  const text = formatSegmentTranscript([
    { startMs: 3000, diarizationLabel: "Você", text: "Bom dia." },
    { startMs: 12000, diarizationLabel: "Participante 1", text: "Olá." },
    {
      startMs: 3661000,
      diarizationLabel: "Participantes",
      text: "Sobreposição.",
    },
  ]);
  assert.equal(
    text,
    "00:00:03\nVocê:\nBom dia.\n\n00:00:12\nParticipante 1:\nOlá.\n\n01:01:01\nParticipantes:\nSobreposição.",
  );
  assert.equal(findTranscriptMatches(text, "Olá").length, 1);
  assert.equal(formatSegmentTranscript([]), "");
});
