/* Athena's Core - landing site interactions
   Everything animates transform/opacity only.
   prefers-reduced-motion: reduce -> static page, no canvas, no GSAP.
   No GSAP on the page -> all content still visible. */
(function () {
  'use strict';

  const REDUCED = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const FINE_POINTER = window.matchMedia('(pointer: fine)').matches;

  /* ── Constellation background ──────────────────────────────── */
  function initStardust() {
    const canvas = document.getElementById('stardust');
    if (!canvas || REDUCED) return;
    const ctx = canvas.getContext('2d');
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    let w = 0;
    let h = 0;
    let stars = [];
    let raf = 0;
    let running = true;
    let mx = 0;
    let my = 0;
    let tmx = 0;
    let tmy = 0;

    function build() {
      const count = Math.max(60, Math.min(150, Math.floor((w * h) / 8500)));
      stars = Array.from({ length: count }, () => ({
        x: Math.random(),
        y: Math.random(),
        r: Math.random() * 1.1 + 0.3,
        a: Math.random() * 0.42 + 0.18,
        tw: Math.random() * Math.PI * 2,
        tws: Math.random() * 0.02 + 0.005,
        bright: Math.random() < 0.09,
        vx: (Math.random() - 0.5) * 0.00005,
        vy: (Math.random() - 0.5) * 0.00004,
      }));
    }

    function resize() {
      w = window.innerWidth;
      h = window.innerHeight;
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
      canvas.style.width = w + 'px';
      canvas.style.height = h + 'px';
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      build();
    }

    function frame() {
      if (!running) return;
      ctx.clearRect(0, 0, w, h);
      mx += (tmx - mx) * 0.04;
      my += (tmy - my) * 0.04;
      ctx.save();
      ctx.translate(mx * 16, my * 11);

      const linkDist = 130;
      const visible = stars.filter((s) => s.a > 0.01);

      for (let i = 0; i < visible.length; i++) {
        for (let j = i + 1; j < visible.length; j++) {
          const a = visible[i];
          const b = visible[j];
          const dx = (a.x - b.x) * w;
          const dy = (a.y - b.y) * h;
          const d2 = dx * dx + dy * dy;
          if (d2 < linkDist * linkDist) {
            const alpha = (1 - Math.sqrt(d2) / linkDist) * 0.13;
            ctx.strokeStyle = 'rgba(201, 162, 75, ' + alpha.toFixed(3) + ')';
            ctx.lineWidth = 1;
            ctx.beginPath();
            ctx.moveTo(a.x * w + a.vx * 1e5, a.y * h + a.vy * 1e5);
            ctx.lineTo(b.x * w + b.vx * 1e5, b.y * h + b.vy * 1e5);
            ctx.stroke();
          }
        }
      }

      for (const s of visible) {
        s.x += s.vx;
        s.y += s.vy;
        if (s.x < -0.02) s.x = 1.02;
        if (s.x > 1.02) s.x = -0.02;
        if (s.y < -0.02) s.y = 1.02;
        if (s.y > 1.02) s.y = -0.02;
        s.tw += s.tws;
        const twinkle = 0.72 + 0.28 * Math.sin(s.tw);
        const alpha = s.a * twinkle;
        const px = s.x * w;
        const py = s.y * h;
        ctx.fillStyle = 'rgba(201, 162, 75, ' + alpha.toFixed(3) + ')';
        ctx.beginPath();
        ctx.arc(px, py, s.r, 0, Math.PI * 2);
        ctx.fill();
        if (s.bright && alpha > 0.3) {
          ctx.strokeStyle = 'rgba(201, 162, 75, ' + (alpha * 0.5).toFixed(3) + ')';
          ctx.lineWidth = 1;
          ctx.beginPath();
          ctx.moveTo(px - s.r * 4, py);
          ctx.lineTo(px + s.r * 4, py);
          ctx.moveTo(px, py - s.r * 4);
          ctx.lineTo(px, py + s.r * 4);
          ctx.stroke();
        }
      }
      ctx.restore();
      raf = requestAnimationFrame(frame);
    }

    window.addEventListener('pointermove', function (e) {
      tmx = (e.clientX / window.innerWidth - 0.5) * 2;
      tmy = (e.clientY / window.innerHeight - 0.5) * 2;
    }, { passive: true });
    window.addEventListener('resize', resize, { passive: true });
    document.addEventListener('visibilitychange', function () {
      if (document.hidden) {
        running = false;
        cancelAnimationFrame(raf);
      } else if (!running) {
        running = true;
        raf = requestAnimationFrame(frame);
      }
    });
    resize();
    raf = requestAnimationFrame(frame);
  }

  /* ── Film: scroll autoplay + play overlay ──────────────────── */
  function initFilm() {
    const film = document.getElementById('product-film');
    if (!film) return;
    const card = film.closest('.film-card');
    const play = document.getElementById('film-play');
    let userPlayed = false;

    play.addEventListener('click', function () {
      userPlayed = true;
      film.muted = false;
      film.play().catch(function () {});
      card.classList.add('is-playing');
    });
    film.addEventListener('play', function () {
      card.classList.add('is-playing');
    });
    film.addEventListener('pause', function () {
      if (userPlayed) card.classList.remove('is-playing');
    });
    film.addEventListener('ended', function () {
      card.classList.remove('is-playing');
    });

    if (REDUCED) return;
    const io = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting && !userPlayed) {
          film.play().catch(function () {});
        }
      });
    }, { threshold: 0.5 });
    io.observe(film);
  }

  /* ── Cursor spotlight on frames / panels ───────────────────── */
  function initSpotlight() {
    if (!FINE_POINTER) return;
    document.querySelectorAll('[data-spot]').forEach(function (el) {
      el.addEventListener('pointermove', function (e) {
        const r = el.getBoundingClientRect();
        el.style.setProperty('--px', e.clientX - r.left + 'px');
        el.style.setProperty('--py', e.clientY - r.top + 'px');
      });
    });
  }

  /* ── Crypto copy-to-clipboard ──────────────────────────────── */
  function initCrypto() {
    document.querySelectorAll('.crypto-code').forEach(function (code) {
      function copy() {
        const text = code.textContent.trim();
        if (!navigator.clipboard) return;
        navigator.clipboard.writeText(text).then(function () {
          code.classList.add('is-copied');
          const prev = code.textContent;
          code.textContent = 'Copied to clipboard';
          setTimeout(function () {
            code.classList.remove('is-copied');
            code.textContent = prev;
          }, 1500);
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

  /* ── What's new: live release notes from the GitHub repo ───── */
  /* Static entries ship in the HTML; this replaces them with real
     GitHub releases as soon as they exist. On failure or when no
     releases are published, the static list stays. */
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
      (r.is_prerelease ? '<span class="release-tag">Beta</span>' : '') +
      '<h3>' + escapeHtml(name) + '</h3>' +
      '<time datetime="' + escapeHtml(r.published_at) + '">' + date + '</time>' +
      '</div>' +
      '<div class="release-body">' + body + '</div>' +
      '<a class="release-link" href="' + escapeHtml(r.html_url) + '" target="_blank" rel="noopener">View release <span aria-hidden="true">↗</span></a>' +
      '</article>'
    );
  }

  function initReleases() {
    const list = document.getElementById('release-list');
    if (!list) return;
    const CACHE_KEY = 'athenas-releases-v1';
    const CACHE_TTL = 10 * 60 * 1000; /* 10 minutes */

    function render(data) {
      const releases = data.filter(function (r) { return !r.draft; }).slice(0, 3);
      if (releases.length === 0) return;
      list.innerHTML = releases.map(renderRelease).join('');
      if (window.ScrollTrigger) {
        ScrollTrigger.getAll()
          .filter(function (t) { return t.trigger && !t.trigger.isConnected; })
          .forEach(function (t) { t.kill(); });
        ScrollTrigger.refresh();
      }
      if (window.gsap && !REDUCED) {
        gsap.fromTo(list.children, { opacity: 0, y: 18 }, {
          opacity: 1, y: 0, stagger: 0.08, duration: 0.6, ease: 'power3.out',
        });
      }
    }

    /* Serve from a fresh cache first so the GitHub API is hit at most
       once per 10 minutes per visitor, not once per page load. */
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
      .catch(function () { /* keep the static entries */ })
      .finally(function () { clearTimeout(timeout); });
  }

  /* ── Nav scrolled state ────────────────────────────────────── */
  function initNav() {
    const header = document.getElementById('site-header');
    if (!header) return;
    function update() {
      header.classList.toggle('is-scrolled', window.scrollY > 8);
    }
    window.addEventListener('scroll', update, { passive: true });
    update();
  }

  /* ── GSAP: hero, reveals, parallax, progress ───────────────── */
  function initGsap() {
    gsap.registerPlugin(ScrollTrigger);

    /* Hero load-in */
    const tl = gsap.timeline({ defaults: { ease: 'power3.out' }, delay: 0.1 });
    tl.from('[data-hero="eyebrow"]', { opacity: 0, y: 14, duration: 0.55 }, 0.05)
      .from('.hero-title .line-inner', { yPercent: 112, duration: 1.05, ease: 'power4.out', stagger: 0.12 }, 0.12)
      .from('[data-hero="sub"]', { opacity: 0, y: 18, duration: 0.6 }, 0.55)
      .from('[data-hero="actions"]', { opacity: 0, y: 18, duration: 0.6 }, 0.68)
      .from('[data-hero="visual"]', { opacity: 0, y: 42, scale: 0.965, duration: 1.15, ease: 'power3.out' }, 0.35);

    /* Scroll reveals */
    gsap.utils.toArray('[data-reveal]').forEach(function (el) {
      gsap.fromTo(el, { opacity: 0, y: 26 }, {
        opacity: 1,
        y: 0,
        duration: 0.9,
        ease: 'power3.out',
        scrollTrigger: { trigger: el, start: 'top 86%', once: true },
      });
    });

    /* Zoom parallax on framed screenshots */
    gsap.utils.toArray('[data-parallax]').forEach(function (el) {
      const v = Math.abs(parseFloat(el.getAttribute('data-parallax')) || 6);
      const fromScale = 1 + v * 0.011;
      gsap.fromTo(el, { scale: fromScale }, {
        scale: 1,
        ease: 'none',
        scrollTrigger: {
          trigger: el.closest('article, .hero-visual, .frame, .feature') || el,
          start: 'top bottom',
          end: 'bottom top',
          scrub: 1,
        },
      });
    });

    /* Scroll progress bar */
    gsap.to('#progress', {
      scaleX: 1,
      ease: 'none',
      scrollTrigger: { start: 0, end: 'max', scrub: 0.4 },
    });

    /* Magnetic CTAs (fine pointers only) */
    if (FINE_POINTER) {
      document.querySelectorAll('.hero-actions .btn, .early-actions .btn').forEach(function (btn) {
        const xTo = gsap.quickTo(btn, 'x', { duration: 0.35, ease: 'power3.out' });
        const yTo = gsap.quickTo(btn, 'y', { duration: 0.35, ease: 'power3.out' });
        btn.addEventListener('mousemove', function (e) {
          const r = btn.getBoundingClientRect();
          xTo((e.clientX - (r.left + r.width / 2)) * 0.22);
          yTo((e.clientY - (r.top + r.height / 2)) * 0.34);
        });
        btn.addEventListener('mouseleave', function () {
          xTo(0);
          yTo(0);
        });
      });
    }

    window.addEventListener('load', function () {
      ScrollTrigger.refresh();
    });
  }

  /* ── No-GSAP fallback: everything visible ──────────────────── */
  function fallbackShow() {
    document.querySelectorAll('[data-reveal]').forEach(function (el) {
      el.classList.add('is-in');
    });
  }

  initStardust();
  initFilm();
  initSpotlight();
  initCrypto();
  initReleases();
  initNav();
  if (window.gsap && !REDUCED) {
    initGsap();
  } else {
    fallbackShow();
  }
})();
