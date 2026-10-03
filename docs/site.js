/* Athena's Core - landing site, v4
   Vanilla JS only. IntersectionObserver reveals, film overlay,
   crypto copy, live release notes. No GSAP. Reduced motion ->
   everything simply visible; reveal transitions are transform-
   and opacity-only via CSS class. */
(function () {
  'use strict';

  const REDUCED = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const HAS_GSAP = typeof window.gsap !== 'undefined' && typeof window.ScrollTrigger !== 'undefined';
  if (!HAS_GSAP || REDUCED) document.documentElement.classList.add('no-gsap');

  /* ── Scroll reveals ───────────────────────────────────────── */
  function initReveals() {
    const els = document.querySelectorAll(HAS_GSAP
      ? '.feature, .tenets li, .support-row, .plate'
      : '.block-head, .plate, .feature, .tenets li, .release, .early, .support-row, .material, .interlude-copy');
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
    const counter = document.getElementById('frame-counter');
    if (counter) {
      const fps = parseInt(counter.dataset.fps, 10) || 25;
      film.addEventListener('timeupdate', function () {
        const f = Math.round(film.currentTime * fps);
        counter.textContent = 'Frame ' + String(f).padStart(4, '0') + ' / ' + String(Math.round((film.duration || 0) * fps)).padStart(4, '0');
      });
    }
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

  /* ── Boot curtain cleanup + parallax photos ─────────────── */
  const boot = document.querySelector('.boot');
  if (boot) boot.addEventListener('animationend', function (e) { if (e.animationName === 'boot-lift') boot.remove(); });

  if (HAS_GSAP && !REDUCED) {
    gsap.registerPlugin(ScrollTrigger);
    window.addEventListener('load', function () { ScrollTrigger.refresh(); });

    /* gold progress hairline */
    const pf = document.getElementById('progress-fill');
    if (pf) {
      ScrollTrigger.create({
        start: 0, end: 'max',
        onUpdate: function (self) { pf.style.transform = 'scaleX(' + self.progress + ')'; }
      });
    }

    /* Scene 1 — title drift over the live panes */
    gsap.fromTo('.livepanes', { y: 40 }, {
      y: -60, ease: 'none',
      scrollTrigger: { trigger: '.livehero', start: 'top top', end: 'bottom top', scrub: true }
    });

    /* Scene 2 — owl interlude: curtain wipe + quote rise, scrubbed */
    const owl = gsap.timeline({
      scrollTrigger: {
        trigger: '.interlude', start: 'top 78%', end: 'top 20%', scrub: 0.5
      }
    });
    owl
      .fromTo('.interlude img', { yPercent: -8, scale: 1.3 }, { yPercent: 4, scale: 1.06, ease: 'none' }, 0)
      .fromTo('.interlude-copy > *', { y: 40, opacity: 0 }, { y: 0, opacity: 1, stagger: 0.35, ease: 'power3.out' }, 0.15);

    /* Scene 3 — feature photos parallax within their rows */
    gsap.utils.toArray('.feature-img img').forEach(function (img) {
      gsap.fromTo(img, { yPercent: -5 }, {
        yPercent: 5, ease: 'none',
        scrollTrigger: { trigger: img.closest('.feature'), start: 'top bottom', end: 'bottom top', scrub: true }
      });
    });

    /* Scene 4 — section headings: tracking settle on entry */
    document.querySelectorAll('.block-head h2, .early-title, .interlude-line').forEach(function (h) {
      gsap.fromTo(h, { letterSpacing: '0.09em', y: 26 }, {
        letterSpacing: '0.01em', y: 0, ease: 'power3.out', duration: 0.9,
        scrollTrigger: { trigger: h, start: 'top 88%', once: true }
      });
    });

    /* Scene 5 — materials cards cascade-wipe */
    gsap.fromTo('.material', { y: 70, opacity: 0 }, {
      y: 0, opacity: 1, stagger: 0.14, ease: 'power3.out', duration: 0.9,
      scrollTrigger: { trigger: '.materials-grid', start: 'top 80%', once: true }
    });

    /* Scene 6 — ledger entries stack in */
    ScrollTrigger.batch('.release', {
      start: 'top 86%', once: true,
      onEnter: function (els) { gsap.fromTo(els, { y: 34, opacity: 0 }, { y: 0, opacity: 1, stagger: 0.12, duration: 0.7, ease: 'power3.out' }); }
    });

    /* Scene 7 — early access: gold field breathes in */
    gsap.fromTo('.early-inner > *', { y: 46, opacity: 0 }, {
      y: 0, opacity: 1, stagger: 0.1, ease: 'power3.out', duration: 0.8,
      scrollTrigger: { trigger: '.early', start: 'top 70%', once: true }
    });
  }

  /* vanilla parallax only when GSAP is absent (GSAP scenes own those elements) */
  if (!HAS_GSAP && !REDUCED) {
    const px = Array.prototype.map.call(
      document.querySelectorAll('.hero-art img, .interlude img'),
      function (img) {
        const fig = img.closest('figure, section');
        return { img: img, host: fig };
      }
    ).filter(function (o) { return o.host; });
    let pTicking = false;
    function parallax() {
      px.forEach(function (o) {
        const r = o.host.getBoundingClientRect();
        const prog = (r.top + r.height / 2 - window.innerHeight / 2) * -0.08;
        o.img.style.translate = '0 ' + prog.toFixed(1) + 'px';
      });
      pTicking = false;
    }
    window.addEventListener('scroll', function () {
      if (!pTicking) { pTicking = true; requestAnimationFrame(parallax); }
    }, { passive: true });
    parallax();
  }

/* ── Live hero: typed panes ─────────────────────────────── */
(function livePanes() {
  const sh = document.getElementById('lp-shell');
  const ag = document.getElementById('lp-agent');
  const bd = document.getElementById('lp-board');
  if (!sh || !ag || !bd) return;

  const SCRIPTS = [
    { el: sh, lines: [
      ['dim', '$ athena open mission/'],
      ['', 'workspace ready  &#183;  3 panes'],
      ['dim', '$ athena chat &quot;review the new plan&quot;'],
      ['gold', '&#9679; athena is reading the workspace&#8230;'],
      ['dim', '$ athena board --open'],
    ], start: 2200, cps: 34 },
    { el: ag, lines: [
      ['gold', '&#9679; builder &mdash; running'],
      ['dim', 'plan parsed &#183; 6 tasks'],
      ['', 'writing src/window/*'],
      ['', 'tests: build-panel &#10003;'],
      ['lapis', 'hand-off &rarr; reviewer'],
    ], start: 3100, cps: 30 },
    { el: bd, lines: [
      ['amber', '&#9679; waiting on you'],
      ['dim', 'todo'],
      ['', '&#9634; design systems pass'],
      ['', '&#9634; icon export QA'],
      ['gold', 'doing'],
      ['', '&#9632; website polish'],
      ['lapis', 'done'],
      ['', '&#9632; window assembly'],
    ], start: 3900, cps: 28 },
  ];

  if (REDUCED) {
    SCRIPTS.forEach(function (sc) {
      sc.el.innerHTML = sc.lines.map(function (l) { return '<span class="' + l[0] + '">' + l[1] + '</span>'; }).join('\n');
    });
    return;
  }

  SCRIPTS.forEach(function (sc) {
    let li = 0;
    function typeLine() {
      if (li >= sc.lines.length) return;
      const cls = sc.lines[li][0], html = sc.lines[li][1];
      const span = document.createElement('span');
      span.className = cls;
      sc.el.appendChild(span);
      const tmp = document.createElement('div');
      tmp.innerHTML = html;
      const text = tmp.textContent || '';
      let ci = 0;
      function tick() {
        ci++;
        span.textContent = text.slice(0, ci);
        if (ci < text.length) {
          setTimeout(tick, 1000 / sc.cps + Math.random() * 18);
        } else {
          if (li < sc.lines.length - 1) sc.el.appendChild(document.createTextNode('\n'));
          li++;
          setTimeout(typeLine, 240 + Math.random() * 320);
        }
      }
      tick();
    }
    setTimeout(typeLine, sc.start);
  });
})();
})();
