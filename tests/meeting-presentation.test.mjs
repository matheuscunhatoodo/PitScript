import assert from "node:assert/strict";
import { test } from "node:test";
import {
  findTranscriptMatches,
  formatDuration,
  formatMeetingDate,
  formatSegmentTranscript,
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
