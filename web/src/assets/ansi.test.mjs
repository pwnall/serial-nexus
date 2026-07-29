// Unit tests for the console's minimal ANSI subset (review UIR-1; design §17's
// "a minimal ANSI subset keeps console escape sequences legible"). Run by
// `node --test`, which the serial-nexus-itest `p8_web_history` gate invokes over every
// `*.test.mjs` in this directory.
//
// `ansi.mjs` is pure by construction — text in, render operations out, no DOM — which
// is what makes the interesting half assertable here rather than in a browser. The
// interesting half is chunking: at serial rates an escape sequence split across two
// `tap.data` notifications is the ordinary case, so every sequence below is tested both
// whole and cut at an arbitrary byte.

import { test } from "node:test";
import assert from "node:assert/strict";
import { newAnsiState, parseAnsi, classOf } from "./ansi.mjs";

/// The plain text an op list would put on screen, ignoring cursor motion — enough to
/// assert "nothing was rendered as litter" without restating the whole op vocabulary.
function textOf(ops) {
  return ops
    .filter((o) => o.t === "text")
    .map((o) => o.s)
    .join("");
}

/// Feed `text` to a fresh parser one `n`-character slice at a time, returning every op.
/// A sequence that survives being cut at *every* offset is a sequence whose state really
/// lives in the state object rather than in the caller's chunk boundaries.
function inSlices(text, n) {
  const st = newAnsiState();
  const ops = [];
  for (let i = 0; i < text.length; i += n) {
    ops.push(...parseAnsi(st, text.slice(i, i + n)));
  }
  return { st, ops };
}

/// Merge adjacent `text` ops carrying the same classes. A chunk boundary legitimately
/// splits one run into two — the parser coalesces within a call, not across calls — so
/// this is the equivalence the chunking test is actually about.
function normalize(ops) {
  const out = [];
  for (const op of ops) {
    const last = out[out.length - 1];
    if (op.t === "text" && last && last.t === "text" && last.cls === op.cls) {
      last.s += op.s;
    } else {
      out.push({ ...op });
    }
  }
  return out;
}

test("plain text is one coalesced run", () => {
  const st = newAnsiState();
  const ops = parseAnsi(st, "hello world");
  assert.deepEqual(ops, [{ t: "text", s: "hello world", cls: "" }]);
  assert.equal(st.unknown, 0);
});

test("newline, carriage return and backspace are operations, not characters", () => {
  const st = newAnsiState();
  const ops = parseAnsi(st, "ab\r\ncd\be");
  assert.deepEqual(
    ops.map((o) => o.t),
    ["text", "cr", "nl", "text", "bs", "text"],
  );
  assert.equal(textOf(ops), "abcde");
});

test("SGR sets, nests and resets the attribute classes", () => {
  const st = newAnsiState();
  const ops = parseAnsi(st, "\x1b[1;31mERR\x1b[32m ok\x1b[0m done");
  assert.deepEqual(ops, [
    { t: "text", s: "ERR", cls: "a-b a-fg1" },
    { t: "text", s: " ok", cls: "a-b a-fg2" },
    { t: "text", s: " done", cls: "" },
  ]);
  assert.equal(classOf(st), "");
  assert.equal(st.unknown, 0);
});

test("bright colours and backgrounds map onto the 16-colour palette", () => {
  const st = newAnsiState();
  const ops = parseAnsi(st, "\x1b[93;44mx\x1b[39;49my");
  assert.deepEqual(ops, [
    { t: "text", s: "x", cls: "a-fg11 a-bg4" },
    { t: "text", s: "y", cls: "" },
  ]);
});

test("a 256-colour selector consumes its sub-parameters instead of re-reading them", () => {
  // `38;5;1` is "foreground = palette index 1". Re-reading the `5` and the `1` as plain
  // SGR codes would have set blink and bold instead — the classic way a half-written
  // parser turns a colour into an attribute.
  const st = newAnsiState();
  const ops = parseAnsi(st, "\x1b[38;5;1mred");
  assert.deepEqual(ops, [{ t: "text", s: "red", cls: "a-fg1" }]);
  assert.equal(st.bold, false);
});

test("erase-to-end-of-line and screen clears become their operations", () => {
  const st = newAnsiState();
  assert.deepEqual(parseAnsi(st, "\x1b[K"), [{ t: "el" }]);
  assert.deepEqual(parseAnsi(st, "\x1b[0K"), [{ t: "el" }]);
  assert.deepEqual(parseAnsi(st, "\x1b[2K"), [{ t: "el-all" }]);
  assert.deepEqual(parseAnsi(st, "\x1b[2J"), [{ t: "clear" }]);
  assert.deepEqual(parseAnsi(st, "\x1b[H"), [{ t: "clear" }]);
  assert.deepEqual(parseAnsi(st, "\x1b[1;1H"), [{ t: "clear" }]);
});

test("a progress bar's carriage returns rewind rather than break the line", () => {
  // The motivating case (UIR-1): with no interpretation this arrives as one run-on line.
  const st = newAnsiState();
  const ops = parseAnsi(st, "  5%\r 50%\r100%\n");
  assert.deepEqual(
    ops.map((o) => o.t),
    ["text", "cr", "text", "cr", "text", "nl"],
  );
});

test("every other CSI, OSC and ESC sequence is consumed and counted, never rendered", () => {
  const st = newAnsiState();
  const ops = parseAnsi(
    st,
    "a\x1b[?25lb\x1b]0;a window title\x07c\x1b[10;20Hd\x1b(Be\x1b7f\x1b[1;2;3~g",
  );
  // Nothing between the markers reaches the screen…
  assert.equal(textOf(ops), "abcdefg");
  // …and the console knows how much of the device's vocabulary it is not speaking.
  assert.equal(st.unknown, 6);
});

test("an OSC string terminated by ST is consumed whole", () => {
  const st = newAnsiState();
  const ops = parseAnsi(st, "x\x1b]2;title\x1b\\y");
  assert.equal(textOf(ops), "xy");
  assert.equal(st.unknown, 1);
});

test("C0 line noise and NUL padding are dropped, not drawn", () => {
  const st = newAnsiState();
  const ops = parseAnsi(st, "a\x00\x07\x0bb\x7fc");
  assert.equal(textOf(ops), "abc");
  assert.equal(st.unknown, 4);
  // A tab is not line noise: a `<pre>` renders it.
  assert.equal(textOf(parseAnsi(newAnsiState(), "a\tb")), "a\tb");
});

test("a sequence split across chunks is still one sequence", () => {
  // The normal case at serial rates, not an edge case.
  const st = newAnsiState();
  assert.deepEqual(parseAnsi(st, "up\x1b"), [{ t: "text", s: "up", cls: "" }]);
  assert.deepEqual(parseAnsi(st, "[1;3"), []);
  assert.deepEqual(parseAnsi(st, "2mgo"), [{ t: "text", s: "go", cls: "a-b a-fg2" }]);
  assert.equal(st.unknown, 0);
});

test("the same input survives being cut at every chunk size", () => {
  const input =
    "\x1b[1;31mboot\x1b[0m: \x1b[32mok\x1b[0m\r\n\x1b]0;title\x07" +
    "  1%\r100%\x1b[K\n\x1b[2Jdone\n";
  const whole = normalize(parseAnsi(newAnsiState(), input));
  for (let n = 1; n <= input.length; n++) {
    const { st, ops } = inSlices(input, n);
    assert.deepEqual(
      normalize(ops),
      whole,
      `chunked by ${n} diverged from the whole-input parse`,
    );
    assert.equal(st.mode, 0, `chunked by ${n} left the machine mid-sequence`);
  }
});

test("a truncated CSI does not swallow the newline that follows it", () => {
  // A device that stops mid-sequence (a reset, a dropped line) must not weld the next
  // line onto this one: the control character ends the sequence and is re-read.
  const st = newAnsiState();
  const ops = parseAnsi(st, "a\x1b[3\nb");
  assert.deepEqual(
    ops.map((o) => o.t),
    ["text", "nl", "text"],
  );
  assert.equal(textOf(ops), "ab");
  assert.equal(st.unknown, 1);
});

test("an unterminated OSC gives up instead of eating the console forever", () => {
  const st = newAnsiState();
  parseAnsi(st, `\x1b]0;${"x".repeat(5000)}`);
  const ops = parseAnsi(st, "back");
  assert.equal(textOf(ops), "back");
});

// Review 37-WEBC-3. The give-up above only fired for a *quiet* unterminated string: the
// STRING→STRING_ESC→STRING cycle a bare ESC drives counted nothing, so a stuck line
// spewing 0x1B held the machine in the string state forever. The console rendered
// nothing, and `unknown` — the one place §17 says the operator can see what was thrown
// away — stayed at exactly zero while it happened.
test("an ESC-dense unterminated string still reaches the give-up bound", () => {
  const st = newAnsiState();
  parseAnsi(st, `\x1b]0;${"\x1bA".repeat(3000)}`);
  assert.ok(
    st.unknown > 0,
    "the machine is still inside the string: the give-up bound counted only the " +
      "characters the device chose not to send",
  );
  // …and ordinary output reaches the screen again, rather than being eaten for the rest
  // of the session. The leading `\n` is a state-independent way back to ground, so this
  // assertion measures the give-up rather than where the ESC run's parity left the
  // machine.
  assert.equal(textOf(parseAnsi(st, "\nback")), "back");
});

test("an absurd CSI parameter run is written off rather than buffered", () => {
  const st = newAnsiState();
  const ops = parseAnsi(st, `\x1b[${"1;".repeat(400)}mtext`);
  assert.deepEqual(ops, [{ t: "text", s: "text", cls: "" }]);
  assert.equal(st.seq, "");
  assert.equal(st.unknown, 1);
});
