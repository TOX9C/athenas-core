/* Athena's Core - landing site, v4
   Vanilla JS only. IntersectionObserver reveals, film overlay,
   crypto copy, live release notes. No GSAP. Reduced motion ->
   everything simply visible; reveal transitions are transform-
   and opacity-only via CSS class. */
(function () {
  'use strict';

  const REDUCED = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  /* ── Scroll reveals ───────────────────────────────────────── */
  function initReveals() {
    const els = document.querySelectorAll('.block-head, .plate, .feature, .tenets li, .release, .early, .support-row');
    if (REDUCED || !('IntersectionObserver' in window)) {
      els.forEach(function (el) { el.classList.add('in'); });
      return;
    }
    const io = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) {
          entry.target.classList.add('in');
          io.unobserve(entry.target);
        }
      });
    }, { rootMargin: '0px 0px -12% 0px', threshold: 0.05 });
    els.forEach(function (el) { el.classList.add('pre'); io.observe(el); });
  }

  /* ── Film play overlay ────────────────────────────────────── */
  function initFilm() {
    const film = document.getElementById('product-film');
    const play = document.getElementById('film-play');
    if (!film || !play) return;
    const wrap = film.parentElement;
    let userPlayed = false;
    play.addEventListener('click', function () {
      userPlayed = true;
      film.muted = false;
      film.play().catch(function () {});
      wrap.classList.add('is-playing');
    });
    film.addEventListener('play', function () { wrap.classList.add('is-playing'); });
    film.addEventListener('pause', function () { if (userPlayed) wrap.classList.remove('is-playing'); });
    film.addEventListener('ended', function () { wrap.classList.remove('is-playing'); userPlayed = false; });
  }

  /* ── Crypto copy-to-clipboard ─────────────────────────────── */
  function initCrypto() {
    document.querySelectorAll('.crypto-code').forEach(function (code) {
      function copy() {
        const text = code.textContent.trim();
        const label = code.querySelector('span');
        const address = label ? text.replace(label.textContent.trim(), '').trim() : text;
        if (!navigator.clipboard) return;
        navigator.clipboard.writeText(address).then(function () {
          code.classList.add('is-copied');
          setTimeout(function () { code.classList.remove('is-copied'); }, 1400);
        }).catch(function () {});
      }
      code.addEventListener('click', copy);
      code.addEventListener('keydown', function (e) {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          copy();
        }
      });
    });
  }

  /* ── Live release notes from GitHub ───────────────────────── */
  function escapeHtml(s) {
    return String(s).replace(/[&<>"']/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c];
    });
  }

  function renderRelease(r) {
    const name = r.name || r.tag_name;
    const date = new Date(r.published_at).toLocaleDateString('en-US', {
      year: 'numeric', month: 'short', day: 'numeric',
    });
    const body = window.marked
      ? window.marked.parse(escapeHtml(r.body || ''))
      : '<p>See the full notes on GitHub.</p>';
    return (
      '<article class="release">' +
      '<div class="release-head">' +
      '<span class="release-tag">' + escapeHtml(r.tag_name) + '</span>' +
      (r.prerelease ? '<span class="release-tag">Beta</span>' : '') +
      '<h3>' + escapeHtml(name) + '</h3>' +
      '<time datetime="' + escapeHtml(r.published_at) + '">' + date + '</time>' +
      '</div>' +
      '<div class="release-body">' + body + '</div>' +
      '<a class="release-link" href="' + escapeHtml(r.html_url) + '" target="_blank" rel="noopener">View release ↗</a>' +
      '</article>'
    );
  }

  function initReleases() {
    const list = document.getElementById('release-list');
    if (!list) return;
    const CACHE_KEY = 'athenas-releases-v2';
    const CACHE_TTL = 10 * 60 * 1000;

    function render(data) {
      const releases = data.filter(function (r) { return !r.draft; }).slice(0, 3);
      if (releases.length === 0) return;
      list.innerHTML = releases.map(renderRelease).join('');
      list.querySelectorAll('.release').forEach(function (el) { el.classList.add('in'); });
    }

    try {
      const raw = localStorage.getItem(CACHE_KEY);
      if (raw) {
        const parsed = JSON.parse(raw);
        if (Date.now() - parsed.t < CACHE_TTL && Array.isArray(parsed.data)) {
          render(parsed.data);
          return;
        }
      }
    } catch (e) { /* ignore */ }

    const controller = new AbortController();
    const timeout = setTimeout(function () { controller.abort(); }, 6000);
    fetch('https://api.github.com/repos/TOX9C/athenas-core/releases?per_page=3', {
      signal: controller.signal,
      headers: { Accept: 'application/vnd.github+json' },
    })
      .then(function (res) {
        if (!res.ok) throw new Error('HTTP ' + res.status);
        return res.json();
      })
      .then(function (data) {
        if (!Array.isArray(data) || data.length === 0) return;
        render(data);
        try {
          localStorage.setItem(CACHE_KEY, JSON.stringify({ t: Date.now(), data: data }));
        } catch (e) { /* ignore */ }
      })
      .catch(function () { /* keep the static entry */ })
      .finally(function () { clearTimeout(timeout); });
  }

  initReveals();
  initFilm();
  initCrypto();
  initReleases();
})();
