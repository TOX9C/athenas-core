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

    function refresh() {
      if (!layer) return;
      var buf = term.buffer.active;
      var vy = buf.viewportY;
      var cell = cellSize(term);
      var screen = term.element.querySelector(".xterm-screen");
      var ox = screen ? screen.offsetLeft : 0;
      var oy = screen ? screen.offsetTop : 0;
      for (var i = images.length - 1; i >= 0; i--) {
        var img = images[i];
        if (img.marker.isDisposed) {
          if (img.url) URL.revokeObjectURL(img.url);
          img.el.remove();
          images.splice(i, 1);
          continue;
        }
        // Images belong to the buffer that created them: an alt-screen
        // switch must hide, not repaint, the normal buffer's overlays.
        if (img.buffer && img.buffer !== term.buffer.active) {
          img.el.style.display = "none";
          continue;
        }
        var y = img.marker.line - vy;
        if (y <= -img.rows || y >= term.rows) {
          img.el.style.display = "none";
          continue;
        }
        img.el.style.display = "";
        img.el.style.left = ox + img.col * cell.w + "px";
        img.el.style.top = oy + y * cell.h + "px";
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
      var finish = function (el, naturalW, naturalH, url) {
        var cell = cellSize(term);
        var cols = params.c ? parseInt(params.c, 10) : Math.max(1, Math.ceil(naturalW / cell.w));
        var rows = params.r ? parseInt(params.r, 10) : Math.max(1, Math.ceil(naturalH / cell.h));
        var buf = term.buffer.active;
        var marker = term.registerMarker(0);
        el.style.cssText = "position:absolute;object-fit:fill;";
        ensureLayer().appendChild(el);
        var entry = { marker: marker, el: el, col: buf.cursorX, rows: rows, cols: cols, url: url, buffer: buf };
        marker.onDispose && marker.onDispose(function () {
          if (entry.url) URL.revokeObjectURL(entry.url);
          el.remove();
          var idx = images.indexOf(entry);
          if (idx !== -1) images.splice(idx, 1);
        });
        images.push(entry);
        refresh();
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
    }

    function b64decode(data) {
      var bin;
      try {
        bin = atob(data.replace(/\s/g, ""));
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
      var flushPlain = function (to) { if (to > from) out.push(bytes.subarray(from, to)); };
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
        // xterm as a grid of missing-glyph boxes.
        if (
          bytes[i] === 0xf4 && bytes[i + 1] === 0x8e &&
          bytes[i + 2] === 0xbb && bytes[i + 3] === 0xae
        ) {
          flushPlain(i);
          i += 4;
          while (i + 1 < bytes.length &&
                 (bytes[i] === 0xcc || bytes[i] === 0xcd) &&
                 bytes[i + 1] >= 0x80 && bytes[i + 1] <= 0xbf) {
            i += 2;
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

    var disposeRender = term.onRender ? term.onRender(refresh) : null;
    var disposeScroll = term.onScroll ? term.onScroll(refresh) : null;
    var onBuf = term.buffer && term.buffer.onBufferChange;
    var disposeBuffer = onBuf ? term.buffer.onBufferChange(refresh) : null;

    return {
      feed: feed,
      dispose: function () {
        clearImages();
        if (layer) layer.remove();
        disposeRender && disposeRender.dispose && disposeRender.dispose();
        disposeScroll && disposeScroll.dispose && disposeScroll.dispose();
        disposeBuffer && disposeBuffer.dispose && disposeBuffer.dispose();
      },
    };
  }

  window.AthenaKitty = { attach: attach };
})();
