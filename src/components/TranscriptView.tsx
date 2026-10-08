import { Fragment, useEffect, useRef } from "react";
import type { TranscriptSegment } from "../types/meeting";
import {
  activeSegmentIndices,
  buildTranscriptBlocks,
  findTranscriptMatches,
} from "../utils/meeting";

type Match = ReturnType<typeof findTranscriptMatches>[number];
function Highlight({
  text,
  offset,
  matches,
  selected,
  activeRef,
}: {
  text: string;
  offset: number;
  matches: Match[];
  selected: number;
  activeRef: React.RefObject<HTMLElement | null>;
}) {
  const visible = matches
    .map((match, index) => ({
      start: Math.max(0, match.start - offset),
      end: Math.min(text.length, match.end - offset),
      index,
    }))
    .filter((match) => match.end > match.start);
  if (!visible.length) return <>{text}</>;
  return (
    <>
      {visible.map((match, index) => (
        <Fragment key={match.index}>
          {text.slice(index ? visible[index - 1].end : 0, match.start)}
          <mark
            ref={match.index === selected ? activeRef : undefined}
            className={
              match.index === selected ? "transcript-match--active" : ""
            }
          >
            {text.slice(match.start, match.end)}
          </mark>
        </Fragment>
      ))}
      {text.slice(visible[visible.length - 1].end)}
    </>
  );
}
export function TranscriptView({
  segments,
  text,
  matches,
  selectedMatch,
  currentTime,
  onSeek,
}: {
  segments: TranscriptSegment[];
  text: string;
  matches: Match[];
  selectedMatch: number;
  currentTime: number;
  onSeek?: (seconds: number) => void;
}) {
  const active = useRef<HTMLElement | null>(null);
  useEffect(() => {
    active.current?.scrollIntoView({ block: "nearest" });
  }, [selectedMatch, matches]);
  if (!segments.length)
    return (
      <div
        className="transcript-text"
        tabIndex={0}
        aria-label="Texto da transcrição"
      >
        <Highlight
          text={text}
          offset={0}
          matches={matches}
          selected={selectedMatch}
          activeRef={active}
        />
      </div>
    );
  const playing = activeSegmentIndices(segments, currentTime);
  return (
    <div className="transcript-document" aria-label="Texto da transcrição">
      {buildTranscriptBlocks(segments).map((block, index) => (
        <section
          key={block.id}
          className={`transcript-block${playing.includes(index) ? " transcript-block--playing" : ""}`}
          aria-label={`${block.time} ${block.diarizationLabel}`}
        >
          <button
            className="timestamp-button"
            disabled={!onSeek}
            aria-label={`Ouvir a partir de ${block.time}`}
            onClick={() => onSeek?.(block.startMs / 1000)}
          >
            <Highlight
              text={block.time}
              offset={block.offset}
              matches={matches}
              selected={selectedMatch}
              activeRef={active}
            />
          </button>
          <div>
            <strong>
              <Highlight
                text={block.diarizationLabel}
                offset={block.labelOffset}
                matches={matches}
                selected={selectedMatch}
                activeRef={active}
              />
            </strong>
            <p>
              <Highlight
                text={block.text}
                offset={block.bodyOffset}
                matches={matches}
                selected={selectedMatch}
                activeRef={active}
              />
            </p>
          </div>
        </section>
      ))}
    </div>
  );
}
