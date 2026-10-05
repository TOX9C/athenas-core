/* Kitty graphics protocol (APC G) + cell-size query responses for the
 * vendored xterm.js build. Stock xterm silently drops APC payloads, so
 * this addon intercepts raw PTY bytes in write_bytes_to_term before they
 * reach the xterm parser: image sequences are stripped, decoded, and
 * painted into an absolutely-positioned overlay layer anchored to buffer
 * markers (which track scroll and dispose when trimmed).
 *
 * ponytail: subset matching what OMP emits — PNG (f=100) and raw RGBA/RGB
 * (f=32/24), transmit+display (a=T), transmit (a=t), placement (a=p),
 * query (a=q), delete-any (a=d, treated as delete-all), multi-chunk (m),
 * (c/r), no-cursor-move (C=1). Unicode placeholders (U+10EEEE runs) are
 * stripped in feed(). Unsupported: z<0 (below text), animation frames,
 * relative offsets.
 */
(function () {
  "use strict";

  var ESC = 0x1b;
  // NBSP bytes (EF BF ... no — U+00A0 is C2 A0): one invisible sentinel per
  // stripped placeholder cell; refresh() finds image blocks by scanning the
  // viewport for these, so overlays anchor to the CELL grid, not to whatever
  // marker the cursor happened to be near at placement time.
  var SENTINEL = new Uint8Array([0xc2, 0xa0]); // U+00A0 no-break space

  // Kitty rowcolumn-diacritics codepoint ranges (UC6 NSM set, 297 entries).
  // A row/column index maps into this table with no plain-ASCII gaps, so
  // testing the decoded codepoint against these ranges is both exact and
  // cheaper than a 297-entry lookup.
  var DIA_RANGES = [
    [0x305, 0x336], [0x337, 0x33a], [0x33b, 0x33c], [0x33d, 0x370],
    [0x483, 0x489], [0x590, 0x5c7], [0x610, 0x61a], [0x64b, 0x665],
    [0x670, 0x670], [0x6d6, 0x6ee], [0x730, 0x74a], [0x7eb, 0x7f3],
    [0x7fd, 0x7fd], [0x816, 0x826], [0x829, 0x82d], [0x951, 0x954],
    [0x957, 0x957], [0x9e2, 0x9e3], [0x9fe, 0x9fe], [0xa3b, 0xa3e],
    [0xa66f, 0xa672], [0xa674, 0xa67d], [0xa69e, 0xa69f], [0xa6f0, 0xa6f1],
    [0xa8e0, 0xa8f1], [0xaab0, 0xaac1], [0xaaf6, 0xaaf6], [0xaab7, 0xaab8],
    [0xaabe, 0xaabf], [0xabec, 0xabed], [0xf82, 0xf87], [0x135d, 0x135f],
    [0x17b2, 0x17b3], [0x17b4, 0x17b6], [0x17b7, 0x17bd], [0x17dd, 0x17dd],
    [0x180b, 0x180e], [0x18a9, 0x18a9], [0x1939, 0x193b], [0x1a17, 0x1a18],
    [0x1a1b, 0x1a1b], [0x1a56, 0x1a56], [0x1a58, 0x1a5e], [0x1a60, 0x1a60],
    [0x1a62, 0x1a62], [0x1a65, 0x1a6c], [0x1a73, 0x1a7c], [0x1ab0, 0x1acd],
    [0x1cd0, 0x1cf9], [0x1dc0, 0x1dff], [0x1de6, 0x1dfe], [0x20d0, 0x20f0],
    [0x2cef, 0x2cf1], [0x2de0, 0x2dff], [0x302a, 0x302d], [0x3099, 0x309a],
    [0xa823, 0xa827], [0xa82c, 0xa82c], [0x10a0f, 0x10a38], [0x1d185, 0x1d244]
  ];

  function isKittyDiacritic(cp) {
    if (cp < 0x300) return false;
    for (var r = 0; r < DIA_RANGES.length; r++) {
      if (cp >= DIA_RANGES[r][0] && cp <= DIA_RANGES[r][1]) return true;
    }
    return false;
  }

  function cellSize(term) {
    var svc = term._core && term._core._renderService;
    var cell = svc && svc.dimensions && svc.dimensions.css && svc.dimensions.css.cell;
    if (cell && cell.width && cell.height) return { w: cell.width, h: cell.height };
    // Fallback: divide the element box by the grid.
    var el = term.element;
    if (el && term.cols && term.rows) {
      return { w: el.clientWidth / term.cols, h: el.clientHeight / term.rows };
    }
    return { w: 9, h: 18 };
  }

  function parseParams(text) {
    var semi = text.indexOf(";");
    var head = semi === -1 ? text : text.slice(0, semi);
    var data = semi === -1 ? "" : text.slice(semi + 1);
    var params = {};
    if (head) {
      head.split(",").forEach(function (kv) {
        var eq = kv.indexOf("=");
        if (eq > 0) params[kv.slice(0, eq)] = kv.slice(eq + 1);
      });
    }
    return { params: params, data: data };
  }

  function attach(term, respond) {
    var doc = term.element && term.element.ownerDocument;
    if (!doc) return { feed: function (b) { return b; }, dispose: function () {} };

    var layer = null;
    var images = []; // {marker, el, rows}
    var transmitted = {}; // id -> {params, data}
    var building = null; // multi-chunk accumulator: {paramsParts, data}
    var capture = null; // bytes of an APC G payload seen so far (mid-sequence)
    var tail = null; // trailing ESC / ESC _ that may start a sequence next chunk
    var pendingDiacritic = false; // dangling placeholder diacritic lead consumed at a chunk end; next chunk's trail byte(s) are skipped

    var paneId = null;
    function logPlace(phase, msg) {
      try {
        if (paneId === null) {
          var m = term.element && term.element.closest &&
            term.element.closest(".xterm-mount");
          paneId = (m && m.getAttribute("data-pane-id")) || "?";
        }
        console.log("[kitty-place] " + paneId + " " + phase + " " + msg);
      } catch (e) {}
    }

    // Settle pending placements and repaint. Exposed to Rust as the
    // __athenaKittySettle expando, invoked from the term.write() callback —
    // the only point in the feed()->write() pipeline where the same chunk's
    // preceding escapes are guaranteed parsed. scheduleSettle() below is the
    // fallback for image-only chunks (nothing reaches the parser, so no
    // write callback ever fires — but then there's also nothing to wait for).
    var settleScheduled = false;
    function settleArmed() {
      settleScheduled = false;
      if (!images.length) return;
      refresh(true);
    }
    function scheduleSettle() {
      if (settleScheduled) return;
      settleScheduled = true;
      var win = doc.defaultView || window;
      (win.requestAnimationFrame || function (f) { return win.setTimeout(f, 16); })(settleArmed);
    }

    function ensureLayer() {
      if (layer) return layer;
      var el = term.element;
      if (getComputedStyle(el).position === "static") el.style.position = "relative";
      layer = doc.createElement("div");
      layer.className = "athena-kitty-layer";
      layer.style.cssText =
        "position:absolute;inset:0;overflow:hidden;pointer-events:none;z-index:4;";
      el.appendChild(layer);
      refresh();
      return layer;
    }

    // refresh(settleNow): settleNow=true means buffer state is known-current
    // (see settleArmed); plain render/scroll refreshes pass false and must
    // NOT capture anchors — they can fire before the write of the chunk that
    // armed an image has been parsed.
    function refresh(settleNow) {
      if (!layer) return;
      var buf = term.buffer.active;
      var vy = buf.viewportY;
      var cell = cellSize(term);
      // Settle armed placements: their images[] record was created by feed()
      // BEFORE xterm parsed this chunk's preceding escapes (feed runs on the
      // raw bytes ahead of term.write's async parser), so buffer identity
      // captured at feed time can be stale. Settling happens only from a
      // post-parse source (term.write callback via __athenaKittySettle, or
      // the render/scroll/timer fallback for image-only chunks that never
      // re-enter the parser) — see settleArmed() below.
      for (var si = 0; si < images.length; si++) {
        var sim = images[si];
        if (settleNow && sim.armed) {
          sim.armed = false;
          sim.buffer = buf;
          sim.reportSettle = true;
        }
      }
      var screen = term.element.querySelector(".xterm-screen");
      var ox = screen ? screen.offsetLeft : 0;
      var oy = screen ? screen.offsetTop : 0;

      // The overlay position comes from the NBSP sentinel cells the feed
      // spliced where placeholders were — the buffer owns scroll/reflow and we
      // just mirror it. Scan the visible viewport for runs of sentinel cells;
      // each contiguous row-run is one image's block.
      var blocks = [];
      var cur = null;
      for (var ly = vy; ly < Math.min(buf.length, vy + term.rows); ly++) {
        var line = buf.getLine(ly);
        var col = -1;
        if (line) col = line.translateToString(true).indexOf(" ");
        if (col >= 0) {
          if (cur && cur.open) {
            cur.rows++;
          } else {
            cur = { top: ly, col: col, rows: 1, open: true };
            blocks.push(cur);
          }
        } else if (cur) {
          cur.open = false;
        }
      }

      for (var i = images.length - 1; i >= 0; i--) {
        var img = images[i];
        // Images belong to the buffer that created them: an alt-screen
        // switch must hide, not repaint, the normal buffer's overlays.
        if (img.buffer && img.buffer !== term.buffer.active) {
          img.el.style.display = "none";
          continue;
        }
        var block = i < blocks.length ? blocks[i] : null;
        if (img.reportSettle) {
          // Anchor at capture (arm) vs first post-parse refresh: armCursor
          // is the still-unparsed cursor cell feed() would have wrongly used.
          img.reportSettle = false;
          logPlace("settle", "pkey=" + img.pkey +
            " arm(buf=" + img.armBuf + " cur=" + img.armCursor + ")" +
            " now(buf=" + buf.type + " cur=" + buf.cursorY + "," + buf.cursorX + ")" +
            (block ? " block(top=" + block.top + " col=" + block.col + " rows=" + block.rows + ")"
                   : " block=none"));
        }
        if (!block) {
          img.el.style.display = "none";
          continue;
        }
        img.el.style.display = "";
        img.el.style.left = ox + block.col * cell.w + "px";
        img.el.style.top = oy + (block.top - vy) * cell.h + "px";
        img.el.style.width = img.cols * cell.w + "px";
        img.el.style.height = img.rows * cell.h + "px";
      }
    }

    function clearImages() {
      images.forEach(function (img) {
        if (img.url) URL.revokeObjectURL(img.url);
        img.el.remove();
      });
      images = [];
    }

    function displayFrom(params, bytes) {
      var f = params.f || "32";
      try {
      // Placement data only — NEVER capture buffer/cursor state here: feed()
      // runs on the raw chunk BEFORE xterm's async parser has consumed the
      // chunk's preceding escapes, so term.buffer.active at this instant is
      // stale during redraw bursts. The record is pushed armed and the
      // anchor is captured in refresh(settleNow=true), reached post-parse
      // via the write callback (__athenaKittySettle) or settleArmed().
      // Overlay positioning itself scans for the NBSP sentinel block spliced
      // where placeholders were — see refresh().
      var armbuf = term.buffer.active;
      var armBuf = armbuf.type;
      var armCursor = armbuf.cursorY + "," + armbuf.cursorX;
      // Kitty semantics: re-emitting a placement with the same i/p replaces
      // the previous one (OMP re-places on every redraw). Drop the stale copy
      // instead of stacking overlays.
      var pkey = (params.i || "0") + ":" + (params.p || "");
      if (params.p) {
        for (var ri = images.length - 1; ri >= 0; ri--) {
          if (images[ri].pkey === pkey) {
            if (images[ri].url) URL.revokeObjectURL(images[ri].url);
            images[ri].el.remove();
            images.splice(ri, 1);
          }
        }
      }
      var finish = function (el, naturalW, naturalH, url) {
        var cell = cellSize(term);
        var cols = params.c ? parseInt(params.c, 10) : Math.max(1, Math.ceil(naturalW / cell.w));
        var rows = params.r ? parseInt(params.r, 10) : Math.max(1, Math.ceil(naturalH / cell.h));
        el.style.cssText = "position:absolute;object-fit:fill;";
        ensureLayer().appendChild(el);
        logPlace("arm", "pkey=" + pkey + " c=" + cols + " r=" + rows +
          " buf=" + armBuf + " cur=" + armCursor);
        images.push({ el: el, rows: rows, cols: cols, url: url, buffer: null,
                      armed: true, armBuf: armBuf, armCursor: armCursor, pkey: pkey });
        // Deferred: the anchor stays armed until a post-parse refresh.
        scheduleSettle();
      };
      if (f === "100") {
        // OMP sends webp; hardcoding image/png kept onload from firing.
        var mime = "image/png";
        if (bytes.length >= 12 && bytes[0] === 0x52 && bytes[1] === 0x49 &&
            bytes[2] === 0x46 && bytes[3] === 0x46 && bytes[8] === 0x57 &&
            bytes[9] === 0x45 && bytes[10] === 0x42 && bytes[11] === 0x50) {
          mime = "image/webp";
        } else if (bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8) {
          mime = "image/jpeg";
        } else if (bytes.length >= 4 && bytes[0] === 0x47 && bytes[1] === 0x49 &&
                   bytes[2] === 0x46) {
          mime = "image/gif";
        }
        var blob = new Blob([bytes], { type: mime });
        var url = URL.createObjectURL(blob);
        var img = new Image();
        img.onload = function () {
          finish(img, img.naturalWidth, img.naturalHeight, url);
        };
        // Unloadable payloads (unknown codec) must not wedge the filter.
        img.onerror = function () { URL.revokeObjectURL(url); };
        img.src = url;
        return;
      }
      var w = parseInt(params.s || "0", 10), h = parseInt(params.v || "0", 10);
      if (!w || !h) return false;
      var canvas = doc.createElement("canvas");
      canvas.width = w;
      canvas.height = h;
      var ctx = canvas.getContext("2d");
      var imgData = ctx.createImageData(w, h);
      if (f === "32") {
        imgData.data.set(bytes.subarray(0, w * h * 4));
      } else { // f=24 RGB -> RGBA
        var d = imgData.data;
        for (var s = 0, t2 = 0; t2 < d.length; s += 3, t2 += 4) {
          d[t2] = bytes[s];
          d[t2 + 1] = bytes[s + 1];
          d[t2 + 2] = bytes[s + 2];
          d[t2 + 3] = 255;
        }
      }
      ctx.putImageData(imgData, 0, 0);
      finish(canvas, w, h);
      } catch (e) {
      }
    }

    function b64decode(data) {
      // OMP's payload can arrive with stray control bytes (ESC from split ST
      // terminators, BEL/C1 from passthrough wrappers) appended to the base64
      // tail — atob rejects the whole string on a single invalid char and the
      // image never paints. Filter to the base64 alphabet; padding keeps at
      // the end, and the alphabet filter drops it harmlessly mid-stream.
      var cleaned = data.replace(/[^A-Za-z0-9+/=]/g, "");
      var bin;
      try {
        bin = atob(cleaned);
      } catch (e) {
        return null;
      }
      var bytes = new Uint8Array(bin.length);
      for (var i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
      return bytes;
    }

    // Returns extra output bytes (cursor moves) to splice into the stream.
    var pendingWrites = [];
    function handle(payloadText) {
      var parsed = parseParams(payloadText);
      var params = parsed.params;
      if (params.m === "1") {
        if (!building) building = { params: params, data: "" };
        building.data += parsed.data;
        return;
      }
      var data = parsed.data;
      if (building) {
        // Final chunk of a multi-chunk stream: its params complete the first
        // chunk's data.
        params = Object.assign({}, building.params, params);
        data = building.data + parsed.data;
        building = null;
      }
      var a = params.a || "t";
      var quiet = parseInt(params.q || "0", 10);
      var idFlag = "i=" + (params.i || "0") + (params.p ? ",p=" + params.p : "");
      var fail = function (msg) {
        if (quiet < 2) respond("\x1b_G" + idFlag + ";EBADMSG:" + msg + "\x1b\\");
      };
      if (a === "q") {
        respond("\x1b_G" + idFlag + ";OK\x1b\\");
        return;
      }
      if (a === "d") {
        // ponytail: any delete clears all overlays; OMP only uses fine-grained
        // deletes for animations it does not emit.
        clearImages();
        if (quiet < 1) respond("\x1b_G" + idFlag + ";OK\x1b\\");
        return;
      }
      var decoded = data ? b64decode(data) : null;
      if (a === "t") {
        var failText = null;
        if (!decoded) {
          // TEMP: self-report into the pane — backend logging has proven
          // unreliable during this debug session.
          var body = parsed.data || "";
          failText = "\r\n[kitty FAIL a=t i=" + (params.i||"0") + " len=" + body.length + " head=" + JSON.stringify(body.slice(0, 50)) + " tail=" + JSON.stringify(body.slice(-30)) + " atobTest=" + (function(){ try { atob(body.slice(0,64)); return "ok"; } catch (e) { return "ERR:" + e; } })() + "]\r\n";
          try { term.write(failText); } catch (_) {}
        }
        if (decoded) transmitted[params.i || "0"] = { params: params, data: decoded };
        if (quiet < 1) respond("\x1b_G" + idFlag + (decoded ? ";OK" : ";EINVAL:empty") + "\x1b\\");
        return;
      }
      if (a === "p") {
        var stored = transmitted[params.i || "0"];
        if (!stored) { fail("ENOENT"); return; }
        var p2 = Object.assign({}, stored.params, params);
        displayFrom(p2, stored.data);
        if (quiet < 1) respond("\x1b_G" + idFlag + ";OK\x1b\\");
        return;
      }
      // a=T (or transmit+display default): store then display.
      if (!decoded) { fail("empty payload"); return; }
      transmitted[params.i || "0"] = { params: params, data: decoded };
      displayFrom(params, decoded);
      if (quiet < 1) respond("\x1b_G" + idFlag + ";OK\x1b\\");
      // Kitty default moves the cursor below the image unless C=1. Only
      // emitted when r is explicit (ponytail: OMP always sizes with c/r;
      // unsized PNGs rely on the sender reserving rows itself).
      if (params.C !== "1" && params.r) {
        pendingWrites.push("\r\n".repeat(parseInt(params.r, 10)));
      }
    }

    function feed(input) {
      var out = [];
      var from = 0;
      var i = 0;
      // Prepend a stashed partial opener, if any.
      var bytes = input;
      if (tail) {
        var merged = new Uint8Array(tail.length + input.length);
        merged.set(tail, 0);
        merged.set(input, tail.length);
        tail = null;
        bytes = merged;
      }
      // Continue a placeholder's combining-diacritic skip across a chunk
      // boundary: consumed bytes from a partial tail. Uses the same decoded
      // codepoint check as the main branch — continuation diacritics are NOT
      // all CC/CD leads.
      if (pendingDiacritic) {
        pendingDiacritic = false;
        while (i < bytes.length) {
          var b0p = bytes[i];
          var ulenp = b0p < 0x80 ? 1 : b0p < 0xe0 ? 2 : b0p < 0xf0 ? 3 : 4;
          if (i + ulenp > bytes.length) break;
          var cpp;
          if (ulenp === 1) cpp = b0p;
          else if (ulenp === 2) cpp = ((b0p & 31) << 6) | (bytes[i + 1] & 63);
          else if (ulenp === 3) cpp = ((b0p & 15) << 12) | ((bytes[i + 1] & 63) << 6) | (bytes[i + 2] & 63);
          else cpp = ((b0p & 7) << 18) | ((bytes[i + 1] & 63) << 12) | ((bytes[i + 2] & 63) << 6) | (bytes[i + 3] & 63);
          if (isKittyDiacritic(cpp)) i += ulenp; else break;
        }
        from = i;
      }
      var flushPlain = function (to) {
        if (to > from) {
          out.push(bytes.subarray(from, to));
        }
      };
      while (i < bytes.length) {
        if (capture) {
          if (bytes[i] === ESC) {
            if (i + 1 >= bytes.length) {
              // Split terminator: assume ST will complete next chunk; keep ESC.
              tail = [ESC];
              i += 1;
              from = i;
              continue;
            }
            if (bytes[i + 1] === 0x5c) { // ST: sequence complete
              var text = "";
              for (var c0 = 0; c0 < capture.length; c0 += 32768) {
                text += String.fromCharCode.apply(
                  null,
                  capture.slice(c0, c0 + 32768)
                );
              }
              handle(text);
              capture = null;
              i += 2;
              from = i;
              continue;
            }
          }
          capture.push(bytes[i]);
          i += 1;
          from = i;
          continue;
        }
        if (
          bytes[i] === ESC &&
          i + 2 < bytes.length &&
          bytes[i + 1] === 0x5f &&
          bytes[i + 2] === 0x47
        ) {
          flushPlain(i);
          capture = [];
          i += 3;
          from = i;
          continue;
        }
        // Strip Kitty unicode placeholder runs: U+10EEEE (F4 8E BB AE) per
        // image cell, each optionally followed by combining diacritics
        // (U+0300–U+036F = CC 80–BF / CD 80–AF). Without this they reach
        // xterm as a grid of missing-glyph boxes. Each stripped cell MUST
        // be replaced by a space: the sender advances its cursor by writing
        // those cells (the whole point of the placeholder protocol), so
        // deleting them outright shifts every later write left onto the
        // image's rows — garbled, interleaved text.
        // Kitty placeholder cell (U+10EEEE + row/col combining diacritics).
        // We splice a NBSP sentinel (U+00A0) per cell instead: invisible in
        // every renderer, but STAYS IN THE BUFFER so refresh() can find the
        // image's actual block position through scroll/reflow — the whole
        // point of U=1 placeholder semantics. Deleting or spacing the cells
        // makes positions drift from the grid; that's the overlay-nowhere bug.
        if (
          bytes[i] === 0xf4 && bytes[i + 1] === 0x8e &&
          bytes[i + 2] === 0xbb && bytes[i + 3] === 0xae
        ) {
          flushPlain(i);
          out.push(SENTINEL);
          i += 4;
          // Consume up to 2 combining codepoints (row + col) following the
          // rune. Each may be 2–4 UTF-8 bytes; a partial sequence at chunk
          // end is caught by pendingDiacritic.
          for (var dmax = 0; dmax < 2; dmax++) {
            if (i >= bytes.length) { pendingDiacritic = true; break; }
            var b0 = bytes[i];
            var ulen = b0 < 0x80 ? 1 : b0 < 0xe0 ? 2 : b0 < 0xf0 ? 3 : 4;
            if (i + ulen > bytes.length) { pendingDiacritic = true; from = i; i = bytes.length; break; }
            var cp;
            if (ulen === 1) cp = b0;
            else if (ulen === 2) cp = ((b0 & 31) << 6) | (bytes[i + 1] & 63);
            else if (ulen === 3) cp = ((b0 & 15) << 12) | ((bytes[i + 1] & 63) << 6) | (bytes[i + 2] & 63);
            else cp = ((b0 & 7) << 18) | ((bytes[i + 1] & 63) << 12) | ((bytes[i + 2] & 63) << 6) | (bytes[i + 3] & 63);
            if (isKittyDiacritic(cp)) { i += ulen; } else break;
          }
          from = i;
          continue;
        }
        // A U+10EEEE placeholder split across chunks: stash the partial
        // prefix so the run is stripped whole next feed().
        if (bytes[i] === 0xf4 && i + 3 >= bytes.length) {
          tail = Array.from(bytes.subarray(i));
          flushPlain(i);
          i = bytes.length;
          from = i;
          continue;
        }
        if (bytes[i] === ESC && i + 1 === bytes.length) {
          tail = [ESC];
          flushPlain(i);
          i += 1;
          from = i;
          continue;
        }
        if (bytes[i] === ESC && i + 2 === bytes.length && bytes[i + 1] === 0x5f) {
          tail = [ESC, 0x5f];
          flushPlain(i);
          i += 2;
          from = i;
          continue;
        }
        i += 1;
      }
      if (!capture) flushPlain(bytes.length);
      // Splice cursor-advance bytes produced by displayed images.
      while (pendingWrites.length) {
        var sfx = pendingWrites.shift();
        var arr = new Uint8Array(sfx.length);
        for (var k = 0; k < sfx.length; k++) arr[k] = sfx.charCodeAt(k);
        out.push(arr);
      }
      if (out.length === 0) return null;
      var total = 0;
      out.forEach(function (c) { total += c.length; });
      var res = new Uint8Array(total);
      var off = 0;
      out.forEach(function (c) { res.set(c, off); off += c.length; });
      return res;
    }

    // Cell/window-size queries kitty terminals answer and OMP may probe.
    // xterm's own responses go through onData -> the PTY input queue already.
    try {
      if (term.parser && term.parser.registerCsiHandler) {
        term.parser.registerCsiHandler({ final: "t" }, function (params) {
          var code = (params && params[0]) || 0;
          if (code === 16) {
            var cell = cellSize(term);
            respond("\x1b[6;" + Math.round(cell.h) + ";" + Math.round(cell.w) + "t");
            return true;
          }
          if (code === 14) {
            var el = term.element;
            respond("\x1b[4;" + el.clientHeight + ";" + el.clientWidth + "t");
            return true;
          }
          if (code === 18) {
            respond("\x1b[8;" + term.rows + ";" + term.cols + "t");
            return true;
          }
          return false;
        });
      }
    } catch (e) {
      /* older xterm: probes simply go unanswered */
    }

    // Wrapped: subscriber callbacks pass event args (onScroll passes a ydisp
    // number) that refresh(settleNow) would misread as settle permission —
    // settle may only come from settleArmed() (write callback / rAF fallback).
    var refreshNoSettle = function () { refresh(false); };
    var disposeRender = term.onRender ? term.onRender(refreshNoSettle) : null;
    var disposeScroll = term.onScroll ? term.onScroll(refreshNoSettle) : null;
    var onBuf = term.buffer && term.buffer.onBufferChange;
    var disposeBuffer = onBuf ? term.buffer.onBufferChange(refreshNoSettle) : null;

    // Post-parse anchor settle: write_bytes_to_term passes this to
    // term.write(data, cb) so refresh(true) runs only after the chunk's
    // preceding escapes have been parsed by xterm.
    term.__athenaKittySettle = settleArmed;

    return {
      feed: feed,
      dispose: function () {
        clearImages();
        if (term.__athenaKittySettle === settleArmed) term.__athenaKittySettle = null;
        if (layer) layer.remove();
        disposeRender && disposeRender.dispose && disposeRender.dispose();
        disposeScroll && disposeScroll.dispose && disposeScroll.dispose();
        disposeBuffer && disposeBuffer.dispose && disposeBuffer.dispose();
      },
    };
  }

  window.AthenaKitty = { attach: attach };
})();
