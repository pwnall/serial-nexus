// The minimal ANSI subset the web console renders (design §17's Implementation stance:
// "a vendored, permissively licensed terminal renderer (or a minimal ANSI subset) keeps
// console escape sequences legible"). This is the *subset* arm of that choice, and the
// arm §17's own "no Node toolchain, no bundler" makes the affordable one: nothing is
// vendored, nothing is added to any dependency list, and the whole interpreter is one
// dependency-free ES module. Until it existed, decoded bytes went into `<pre id="term">`
// as a Text node and every CSI sequence an ordinary Linux or U-Boot console emits
// rendered as visible litter (review UIR-1).
//
// It is deliberately **not** a terminal emulator. There is no grid, no cursor
// addressing, no scroll region, no alternate screen: the console pane is a scrollback
// log, and a renderer that pretended to own an 80×24 grid would be inventing state the
// daemon never sends and the pane cannot honour. What it does is the honest minimum
// that keeps a device console readable:
//
//   * SGR (`CSI … m`) — reset, bold, dim, and the 8/16 foreground and background
//     colours, reported as CSS class names. The class names are all this module knows
//     about presentation; the DOM writer in `app.js` builds the elements, and the
//     palette lives in `app.css`.
//   * `\r` and `\b`, which move the cursor *within the current line*. A progress bar is
//     the motivating case and it is the one thing a bare `<pre>` gets loudly wrong: a
//     carriage return there neither overwrites nor breaks, so the bar arrives as one
//     unreadable run-on line.
//   * `CSI K` erase-to-end-of-line, `CSI 2J` / `CSI H` screen clear.
//   * Every other CSI, OSC, DCS and ESC sequence **consumed and counted**, never
//     rendered. Counting rather than dropping in silence is §5's honesty applied to the
//     one surface where the operator cannot otherwise see what was thrown away:
//     `state.unknown` is a live tally of how much of the device's vocabulary this
//     subset is not speaking.
//
// **The machine survives a chunk boundary.** At serial rates an escape sequence split
// across two `tap.data` notifications is the normal case, not the edge case — the
// daemon coalesces on its own schedule and knows nothing about sequence boundaries. So
// this is a resumable state machine: `parseAnsi()` is called once per decoded chunk
// with the same state object, and a sequence still in flight at the end of a chunk just
// leaves the machine in the state it reached.
//
// **It never touches the byte log.** The parser reads *decoded text* and emits render
// operations. `bytesOf(history)` stays the raw device stream the export and the OPFS
// record are meant to be (§15.32): the export is a device log, not a transcript. Only
// the screen interprets.

/// The longest CSI parameter/intermediate run collected before the sequence is written
/// off. A device that emits `ESC [` followed by megabytes of digits is malfunctioning;
/// buffering them would be the client's problem rather than the device's.
const MAX_SEQ = 64;

/// The longest OSC/DCS string swallowed before the machine gives up and returns to
/// ground — counted over *every* character the string consumes, ESC included, since a
/// bound only the well-behaved half of the input advances is not a bound (37-WEBC-3).
/// Without it, one unterminated `ESC ]` makes the console silently eat everything the
/// device says for the rest of the session.
const MAX_STRING = 4096;

const GROUND = 0;
const ESC_SEEN = 1;
const CSI_SEQ = 2;
const STRING = 3;
const STRING_ESC = 4;
const SKIP_ONE = 5;

/// A fresh parser. `unknown` counts sequences consumed without being interpreted; the
/// rest is machine state and the current SGR attributes, all of which must persist
/// across chunks.
export function newAnsiState() {
  return {
    mode: GROUND,
    seq: "",
    seqOver: false,
    strLen: 0,
    bold: false,
    dim: false,
    fg: null,
    bg: null,
    unknown: 0,
  };
}

/// The CSS classes for the current SGR attributes — `""` when everything is default, so
/// the common case costs the DOM writer a bare Text node rather than a `<span>`.
export function classOf(st) {
  let c = "";
  if (st.bold) c += "a-b ";
  if (st.dim) c += "a-dim ";
  if (st.fg !== null) c += `a-fg${st.fg} `;
  if (st.bg !== null) c += `a-bg${st.bg} `;
  return c.trim();
}

/// Interpret one decoded chunk, returning the render operations it produced:
///
///   `{ t: "text", s, cls }`  write `s` at the cursor wearing CSS classes `cls`
///   `{ t: "nl" }`            commit the current line and begin a new one
///   `{ t: "cr" }`            cursor to column 0 of the current line
///   `{ t: "bs" }`            cursor left one column
///   `{ t: "el" }`            erase from the cursor to the end of the line (`CSI 0 K`)
///   `{ t: "el-all" }`        erase the whole line (`CSI 2 K`)
///   `{ t: "clear" }`         clear the rendered screen (`CSI 2 J`, `CSI H`)
///
/// Adjacent characters with the same attributes are coalesced into one `text` op, so a
/// 64 KiB chunk of ordinary output costs one operation, not 65 536.
export function parseAnsi(st, text) {
  const ops = [];
  let run = "";
  let runCls = "";
  const flush = () => {
    if (run) {
      ops.push({ t: "text", s: run, cls: runCls });
      run = "";
    }
  };
  // The attributes are captured when a run *starts*, so an SGR change mid-chunk simply
  // ends the previous run: no operation has to carry the state itself.
  const put = (ch) => {
    if (!run) runCls = classOf(st);
    run += ch;
  };
  const emit = (op) => {
    flush();
    ops.push(op);
  };

  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    switch (st.mode) {
      case GROUND: {
        if (ch === "\x1b") {
          flush();
          st.mode = ESC_SEEN;
        } else if (ch === "\n") {
          emit({ t: "nl" });
        } else if (ch === "\r") {
          emit({ t: "cr" });
        } else if (ch === "\b") {
          emit({ t: "bs" });
        } else if (ch === "\t") {
          put(ch); // a `<pre>` renders tabs; nothing to interpret
        } else if (ch < " " || ch === "\x7f") {
          // The rest of C0 plus DEL: NUL padding, BEL, VT, FF and the line-noise a
          // half-connected UART produces. Rendering them is litter; counting them is
          // the honest alternative to pretending they were not there.
          st.unknown += 1;
        } else {
          put(ch);
        }
        break;
      }

      case ESC_SEEN: {
        if (ch === "[") {
          st.mode = CSI_SEQ;
          st.seq = "";
          st.seqOver = false;
        } else if (ch === "]" || ch === "P" || ch === "X" || ch === "^" || ch === "_") {
          // OSC, DCS, SOS, PM, APC: string-valued sequences, terminated by BEL or ST.
          st.mode = STRING;
          st.strLen = 0;
        } else if (
          ch === "(" ||
          ch === ")" ||
          ch === "*" ||
          ch === "+" ||
          ch === "#" ||
          ch === "%"
        ) {
          st.mode = SKIP_ONE; // a two-byte designator; its argument is the next byte
        } else {
          // `ESC 7`, `ESC =`, `ESC c`, … — one byte and done.
          st.unknown += 1;
          st.mode = GROUND;
        }
        break;
      }

      case CSI_SEQ: {
        if (ch >= "\x40" && ch <= "\x7e") {
          st.mode = GROUND;
          const seq = st.seq;
          const over = st.seqOver;
          st.seq = "";
          st.seqOver = false;
          if (over) st.unknown += 1;
          else handleCsi(st, seq, ch, flush, emit);
        } else if (ch >= " " && ch <= "?") {
          if (st.seq.length < MAX_SEQ) st.seq += ch;
          else st.seqOver = true;
        } else {
          // A control character inside a sequence: the device truncated it. Abandon the
          // sequence and re-read this character in ground rather than swallowing it —
          // eating a `\n` here would silently weld two lines together.
          st.unknown += 1;
          st.mode = GROUND;
          st.seq = "";
          st.seqOver = false;
          i -= 1;
        }
        break;
      }

      case STRING: {
        // Every character the string swallows counts toward the give-up bound, the ESC
        // that hands off to STRING_ESC included. Counting only the plain ones let
        // ESC-dense input — a stuck line spewing 0x1B — cycle STRING→STRING_ESC→STRING
        // without ever advancing `strLen`, so `MAX_STRING` never fired: the console
        // rendered nothing for the rest of the session, the honesty tally that exists to
        // say so never moved either, and once ordinary bytes resumed a further
        // `MAX_STRING` of them were still swallowed (review 37-WEBC-3).
        st.strLen += 1;
        if (ch === "\x07") {
          st.unknown += 1;
          st.mode = GROUND;
        } else if (ch === "\x1b") {
          st.mode = STRING_ESC;
        } else if (st.strLen > MAX_STRING) {
          st.unknown += 1;
          st.mode = GROUND;
        }
        break;
      }

      case STRING_ESC: {
        st.strLen += 1;
        if (ch === "\\") {
          st.unknown += 1;
          st.mode = GROUND;
        } else if (st.strLen > MAX_STRING) {
          // The give-up has to live on both string states, or a run of nothing but ESC
          // reaches the bound in one state and returns to the other before acting on it.
          st.unknown += 1;
          st.mode = GROUND;
        } else {
          st.mode = STRING; // a bare ESC inside the string; still inside it
        }
        break;
      }

      case SKIP_ONE: {
        st.unknown += 1;
        st.mode = GROUND;
        break;
      }

      default:
        st.mode = GROUND;
        break;
    }
  }
  flush();
  return ops;
}

/// Dispatch one complete CSI sequence. `seq` is everything between `ESC [` and the final
/// byte; `final` is the final byte.
function handleCsi(st, seq, final, flush, emit) {
  // `CSI ? …` and friends are private-parameter sequences: cursor visibility, the
  // alternate screen, bracketed paste, mouse reporting. None of them mean anything to a
  // scrollback log, and guessing at them is how a renderer starts lying.
  if (seq !== "" && seq[0] >= "<" && seq[0] <= "?") {
    st.unknown += 1;
    return;
  }
  const params = seq.split(";").map((p) => {
    const n = p === "" ? 0 : parseInt(p, 10);
    return Number.isFinite(n) ? n : 0;
  });
  switch (final) {
    case "m":
      flush();
      applySgr(st, params);
      return;
    case "K":
      if (params[0] === 0) emit({ t: "el" });
      else if (params[0] === 2) emit({ t: "el-all" });
      else st.unknown += 1; // `CSI 1 K` erases *before* the cursor; no column model
      return;
    case "J":
      // 0 and 1 are partial screen erases, which need a grid to mean anything.
      if (params[0] === 2 || params[0] === 3) emit({ t: "clear" });
      else st.unknown += 1;
      return;
    case "H":
    case "f":
      // Cursor home. With no grid there is nowhere to home *to*, but the sequence a
      // device actually clears with is `ESC[H ESC[2J` or `ESC[2J ESC[H`, so honouring
      // the home-with-no-argument form as a clear is what makes those legible. Anything
      // addressing a real cell is consumed rather than faked.
      if (seq === "" || seq === "1" || seq === "1;1") emit({ t: "clear" });
      else st.unknown += 1;
      return;
    default:
      st.unknown += 1;
      return;
  }
}

/// Fold one `CSI … m` parameter list into the attribute state.
function applySgr(st, params) {
  for (let i = 0; i < params.length; i++) {
    const p = params[i];
    if (p === 0) {
      st.bold = false;
      st.dim = false;
      st.fg = null;
      st.bg = null;
    } else if (p === 1) {
      st.bold = true;
    } else if (p === 2) {
      st.dim = true;
    } else if (p === 22) {
      st.bold = false;
      st.dim = false;
    } else if (p >= 30 && p <= 37) {
      st.fg = p - 30;
    } else if (p === 39) {
      st.fg = null;
    } else if (p >= 40 && p <= 47) {
      st.bg = p - 40;
    } else if (p === 49) {
      st.bg = null;
    } else if (p >= 90 && p <= 97) {
      st.fg = p - 90 + 8;
    } else if (p >= 100 && p <= 107) {
      st.bg = p - 100 + 8;
    } else if (p === 38 || p === 48) {
      // Extended colour. Its sub-parameters have to be *skipped*, not re-read as
      // attributes, or `38;5;1` would set bold-ish nonsense from the `5`. The first 16
      // indices are the palette this module already has; anything past them falls back
      // to the default rather than being approximated.
      const key = p === 38 ? "fg" : "bg";
      if (params[i + 1] === 5) {
        const n = params[i + 2];
        st[key] = n >= 0 && n < 16 ? n : null;
        if (!(n >= 0 && n < 16)) st.unknown += 1;
        i += 2;
      } else if (params[i + 1] === 2) {
        st[key] = null;
        st.unknown += 1;
        i += 4;
      } else {
        st.unknown += 1;
        i = params.length;
      }
    } else {
      st.unknown += 1;
    }
  }
}
