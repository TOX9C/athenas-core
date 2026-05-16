// xterm.js bootstrap script — loaded by ensure_xterm_bootstrap()
// Dynamically loads xterm.js + addons from CDN, then exposes terminal helpers.

window.__athenaTerminals = new Map()
window.__athenaFitAddons = new Map()
window.__athenaNextHandle = 0
window.__athenaXtermReady = false
window.__athenaXtermQueue = []

function __athenaLoadScript(src) {
  return new Promise(function (resolve, reject) {
    var s = document.createElement('script')
    s.src = src
    s.onload = resolve
    s.onerror = function () {
      reject(new Error('Failed to load ' + src))
    }
    document.head.appendChild(s)
  })
}

function __athenaLoadCSS(href) {
  var link = document.createElement('link')
  link.rel = 'stylesheet'
  link.href = href
  document.head.appendChild(link)
}

;(function () {
  // Load xterm CSS
  __athenaLoadCSS('https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/css/xterm.min.css')

  // Load scripts sequentially: xterm core first, then addons
  __athenaLoadScript('https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/lib/xterm.min.js')
    .then(function () {
      return __athenaLoadScript(
        'https://cdn.jsdelivr.net/npm/@xterm/addon-fit@0.10.0/lib/addon-fit.min.js',
      )
    })
    .then(function () {
      return __athenaLoadScript(
        'https://cdn.jsdelivr.net/npm/@xterm/addon-web-links@0.11.0/lib/addon-web-links.min.js',
      )
    })
    .then(function () {
      window.__athenaXtermReady = true
      // Process any queued create calls
      var queue = window.__athenaXtermQueue
      window.__athenaXtermQueue = []
      for (var i = 0; i < queue.length; i++) {
        queue[i]()
      }
    })
    .catch(function (err) {
      console.warn('[athena] xterm.js load failed:', err)
    })
})()

window.__athenaCreateTerminal = function (container, themeStr) {
  if (!window.__athenaXtermReady || !window.Terminal) {
    console.warn('[athena] xterm.js not ready yet, queuing create')
    return -1
  }

  var theme =
    themeStr === 'light'
      ? {
          background: '#fafafa',
          foreground: '#383a42',
          cursor: '#526eff',
          selectionBackground: '#3e4451',
          black: '#383a42',
          red: '#e45649',
          green: '#50a14f',
          yellow: '#c18401',
          blue: '#4078f2',
          magenta: '#a626a4',
          cyan: '#0184bc',
          white: '#fafafa',
        }
      : {
          background: '#0b0e13',
          foreground: '#c8ccd4',
          cursor: '#528bff',
          selectionBackground: '#3e4451',
          black: '#1e2127',
          red: '#e06c75',
          green: '#98c379',
          yellow: '#e5c07b',
          blue: '#61afef',
          magenta: '#c678dd',
          cyan: '#56b6c2',
          white: '#abb2bf',
        }

  var term = new window.Terminal({
    theme: theme,
    fontFamily: "'JetBrains Mono', 'Fira Code', 'SF Mono', Menlo, monospace",
    fontSize: 13,
    lineHeight: 1.35,
    cursorBlink: true,
    cursorStyle: 'bar',
    allowProposedApi: true,
    scrollback: 10000,
    convertEol: true,
  })

  var fitAddon = new window.FitAddon.FitAddon()
  term.loadAddon(fitAddon)

  if (window.WebLinksAddon) {
    var webLinksAddon = new window.WebLinksAddon.WebLinksAddon()
    term.loadAddon(webLinksAddon)
  }

  term.open(container)
  fitAddon.fit()

  var handle = String(window.__athenaNextHandle++)
  window.__athenaTerminals.set(handle, term)
  window.__athenaFitAddons.set(handle, fitAddon)
  return handle
}

window.__athenaWriteTerminal = function (handle, data) {
  var term = window.__athenaTerminals.get(String(handle))
  if (term) {
    term.write(data)
  }
}

window.__athenaOnTerminalData = function (handle, callback) {
  var term = window.__athenaTerminals.get(String(handle))
  if (term) {
    term.onData(function (data) {
      callback(data)
    })
  }
}

window.__athenaFitTerminal = function (handle) {
  var fitAddon = window.__athenaFitAddons.get(String(handle))
  if (fitAddon) {
    fitAddon.fit()
  }
}

window.__athenaDisposeTerminal = function (handle) {
  var key = String(handle)
  var term = window.__athenaTerminals.get(key)
  if (term) {
    term.dispose()
    window.__athenaTerminals.delete(key)
    window.__athenaFitAddons.delete(key)
  }
}

window.__athenaGetTerminalSize = function (handle) {
  var term = window.__athenaTerminals.get(String(handle))
  if (term) {
    return [term.cols, term.rows]
  }
  return [80, 24]
}

window.__athenaResizeTerminal = function (handle, cols, rows) {
  var term = window.__athenaTerminals.get(String(handle))
  if (term) {
    term.resize(cols, rows)
  }
}

// Attach a custom keydown event handler to the terminal.
// The handler function receives a KeyboardEvent and must return:
//   true  -> xterm.js processes the key normally (e.g. typing chars)
//   false -> xterm.js ignores the key, allowing it to bubble up to the app
window.__athenaAttachCustomKeyEventHandler = function (handle, handler) {
  var term = window.__athenaTerminals.get(String(handle))
  if (term) {
    term.attachCustomKeyEventHandler(handler)
  }
}
