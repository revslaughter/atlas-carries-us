// Source maps: decodes the "mappings" of a version 3 source map and maps a
// position in generated code back to its source. svelte2tsx writes one map
// per component; the adapter uses it to give every location the line in the
// .svelte file. The decoder lives here, in about forty lines, so that the
// adapter needs no source-map package of its own.

/** One mapped position in a generated line: its column, and the line and column in the source that it came from. All count from 0. */
export interface Segment {
  /** The column in the generated line. */
  col: number;
  /** The line in the source. */
  srcLine: number;
  /** The column in the source line. */
  srcCol: number;
}

/** The 64 characters of Base64, in order; the index of a character gives its value. */
const BASE64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
/** The value of each Base64 character. */
const DIGIT = new Map([...BASE64].map((c, i) => [c, i]));

/** Decodes the Base64 VLQ numbers of one segment: five bits per character, a continuation bit, and the sign in the lowest bit. */
export function decodeVLQ(s: string): number[] {
  const out: number[] = [];
  let value = 0;
  let shift = 0;
  for (const c of s) {
    const d = DIGIT.get(c);
    if (d === undefined) throw new Error(`the source map holds "${c}", which Base64 VLQ does not use`);
    value += (d & 31) * 2 ** shift;
    if (d & 32) {
      shift += 5;
      continue;
    }
    const mag = Math.floor(value / 2);
    out.push(value % 2 && mag ? -mag : mag);
    value = 0;
    shift = 0;
  }
  if (shift) throw new Error("the source map ends inside a number");
  return out;
}

/** Decodes a mappings string into the segments of each generated line, sorted by column. Segments that name no source drop out. */
export function decodeMappings(mappings: string): Segment[][] {
  const lines: Segment[][] = [];
  let srcLine = 0;
  let srcCol = 0;
  for (const text of mappings.split(";")) {
    const line: Segment[] = [];
    let col = 0;
    for (const seg of text.split(",")) {
      if (!seg) continue;
      const f = decodeVLQ(seg);
      col += f[0];
      if (f.length < 4) continue;
      srcLine += f[2];
      srcCol += f[3];
      line.push({ col, srcLine, srcCol });
    }
    lines.push(line.sort((a, b) => a.col - b.col));
  }
  return lines;
}

/** Maps positions in generated code back to the source, through the decoded mappings of one source map. */
export class SourceMap {
  /** The segments of each generated line. */
  private readonly lines: Segment[][];

  /** Decodes the mappings of a version 3 source map. */
  constructor(mappings: string) {
    this.lines = decodeMappings(mappings);
  }

  /**
   * Returns the source line and column of a generated position, counting
   * from 0. A position with no segment of its own takes the source position
   * of the nearest earlier segment on its line; a line with none before the
   * position gives undefined.
   */
  original(line: number, col: number): { line: number; col: number } | undefined {
    const segs = this.lines[line];
    if (!segs) return undefined;
    let best: Segment | undefined;
    for (const s of segs) {
      if (s.col > col) break;
      best = s;
    }
    return best && { line: best.srcLine, col: best.srcCol };
  }
}
