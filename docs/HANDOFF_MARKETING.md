# HANDOFF — Athena's Core Marketing & Launch
*Prepared by: Apollo's assistant (Hermes) — August 2026*

This handoff gives you (the agent taking over) everything you need: what the product is, what's already done, what's left, which tools to use, and exactly what to post where. Do NOT re-do anything marked ✅ — verify the repo state first, then execute the remaining items.

---

## 1. THE PRODUCT

**Athena's Core** — a native desktop IDE for AI-assisted software development.

| | |
|---|---|
| **GitHub** | https://github.com/TOX9C/athenas-core |
| **Website** | https://tox9c.github.io/athenas-core/ (GitHub Pages, auto-deploys from `docs/` via `.github/workflows/pages.yml`) |
| **Owner** | TOX9C (Apollo — solo dev, university student in Iraq) |
| **Stack** | Rust + Tauri 2 backend (133 IPC commands) · Dioxus WASM frontend (85+ components) · SQLite |
| **Binary size** | ~15MB — no Electron, no bundled Chromium, no Node runtime |
| **Platform** | macOS 13+ on Apple Silicon (only release target; Linux/Windows in progress) |
| **License** | MIT (added 2026-08) |
| **Version** | v0.3.0 |

**Key features:** multi-pane terminal (1×1 to 4×4, ANSI/VT100, OSC 633), workspace-aware AI chat (Claude/OpenAI/NVIDIA NIM/LM Studio local), agent swarm (Coordinator/Builder/Scout/Reviewer on mailbox bus), Kanban task board with agent assignment, plugin system (JSON manifests, capability scoping), 16 themes, command palette (Cmd+K), embedded browser, mobile companion (LAN PWA).

**Story angles (use these in all posts):**
1. **Solo dev from Iraq** built a full AI-assisted IDE — nobody-funding, no-big-team underdog story
2. **Anti-Electron**: 15MB vs 350MB (VS Code), fraction of RAM — "native where it counts"
3. **Local-first / privacy**: local models, SQLite, no telemetry, self-hosted friendly
4. **Tech rarity**: pushing Dioxus 0.7 + Tauri 2 as a serious desktop stack (r/rust will care about this specifically)

---

## 2. CURRENT STATE — DO NOT REDO

✅ **Done and pushed to GitHub:**
- MIT `LICENSE` file
- README: badges (MIT/Platform/Rust/Stars/Donate), Support the Developer section with real wallet addresses + NowPayments link
- Website (`docs/index.html` + `docs/styles.css`): Support/Donate section with crypto cards + "Buy me a coffee" button → https://nowpayments.io/donation/tox9c
- In-app (Settings → About): "Support the Developer" block with wallet addresses + donate link
- Repo metadata: description set, homepage set, 13 topics (`rust`, `tauri`, `ide`, `ai-assisted`, `terminal`, `dioxus`, `native`, `multi-agent`, `developer-tools`, `desktop-app`, `wasm`, `open-source`, `llm`)
- Old "Promotional Ad v1" release deleted
- Draft release `v0.3.0` created with release notes — **NEEDS the `.dmg` uploaded + published** (see §5)

**Wallet / donation addresses (already embedded everywhere — do not change unless Apollo provides new ones):**
- BTC: `bc1qn8ehwc7rxlpgvljztr5k6npqf307xq00dqatf8`
- ETH/USDT/USDC (ERC-20): `0x4260456e1dbdc880d69d75949726953215a93586`
- USDT (TRC-20): `TSBUpAreTjmUscbUbf4L1wkX1fvvJvSRGW`
- NowPayments donate page: https://nowpayments.io/donation/tox9c

**⚠️ CRITICAL CONSTRAINT — payments in Iraq:** Do NOT recommend or set up Buy Me a Coffee, Ko-fi, Patreon, GitHub Sponsors, Gumroad, Liberapay, Stripe, or PayPal **receiving** — none support payouts to Iraq. The NowPayments + crypto-wallet setup is intentionally the donation path. If Apollo asks about a new payment platform, verify payout-to-Iraq support BEFORE promising anything.

---

## 3. THE TASK — MARKETING & LAUNCH

Apollo wants people to **use** the app. The app is free/MIT. Success = downloads, stars, and feedback. Donations are secondary (infrastructure exists; don't push hard).

Drafted posts are in `MARKETING_POSTS.md` (in this repo, root). They're ready to post — review, adjust tone, then publish. Do NOT create new posts from scratch without reading the drafts first; they encode the right angles.

### Target platforms & tools

| Platform | Draft exists | Tool to use | Notes |
|---|---|---|---|
| **Hacker News (Show HN)** | ✅ yes | Web browser (news.ycombinator.com) | The #1 priority. Best posting window: 8–10am US Eastern (≈3–5pm Baghdad, evening Silicon Valley). Needs an HN account with a little karma — if fresh, comment on 2–3 technical threads first, wait a few hours, then post. |
| **Reddit — r/rust** | ✅ yes | `rdt` CLI or browser | Technical deep-dive audience. Post same day as HN, ~2h later. Include crate table + ask specific questions (Dioxus/Tauri claims, ANSI emulator, swarm architecture). |
| **Reddit — r/programming** | ✅ yes | `rdt` CLI or browser | Anti-Electron angle. Never post to 2 defacto-aggressive subs simultaneously; stagger 2–3h. |
| **Reddit — r/selfhosted** | ✅ yes | `rdt` CLI or browser | Local-first/privacy angle. Post next day (not same day). |
| **Twitter / X** | ❌ not drafted | `xurl` CLI | Short thread: 3–5 tweets (screenshot + size claim + agent swarm GIF + repo link). If Apollo lacks followers, X has low impact — deprioritize. |
| **dev.to** | ❌ not drafted | browser | Long-form "How I built a 15MB IDE in Rust" article. Reuse README content + architecture diagram. Good SEO evergreen. Lower priority. |
| **Product Hunt** | ❌ not drafted | browser | Only AFTER the GitHub release is live with a downloadable .dmg. Medium priority. |
| **Lobsters** | ❌ not drafted | browser (invite needed) | Same tech audience as HN; only if Apollo has/login can get an invite. Low priority. |

### Posting order (recommended sequence)
1. **Day 1:** Show HN (morning EST) → r/rust (2–3h later) → r/programming (2–3h after that, if r/rust is getting traction)
2. **Day 2:** r/selfhosted
3. **Day 3+:** dev.to article, then decide on X/Product Hunt based on early feedback

### Reply & engagement obligations
- **Watch your inbox/notifications on HN and Reddit for 48h after posting.** Reply to every substantive comment. Common pushback to prepare answers for: "Why not just use VS Code + extension?" "Why Dioxus and not Leptos/Yew?" "Mac-only?" "How does the swarm not burn tokens?" "How are permissions handled for agents?"
- Never be defensive. Apollo's voice: direct, humble-ish, technical. Refer to himself as "a solo dev" / "I built this."

---

## 4. VERIFY BEFORE POSTING (10-min checklist)

Run these to confirm the repo is in the state this handoff describes:

```bash
export HOME=/Users/apollo   # gh/git need this; sandbox blocks default HOME
gh repo view TOX9C/athenas-core --json description,licenseInfo,repositoryTopics
gh api repos/TOX9C/athenas-core/contents/LICENSE --jq '.name'
gh api repos/TOX9C/athenas-core/readme --jq '.content' | base64 -d | grep -i "nowpayments\|Support the Developer"
gh api repos/TOX9C/athenas-core/contents/docs/index.html --jq '.content' | base64 -d | grep -c "nowpayments"
gh api repos/TOX9C/athenas-core/contents/frontend/src/components/settings/settings_sections.rs --jq '.content' | base64 -d | grep -c "TSBUpAreTjmUscbUbf4L1wkX1fvvJvSRGW"
gh release view v0.3.0 --repo TOX9C/athenas-core --json isDraft,assets
```

If anything greps 0 or looks missing, fix it before posting. Do not post marketing for a repo that looks abandoned.

---

## 5. REMAINING NON-MARKETING ITEM

- **Release needs the `.dmg`:** the `v0.3.0` draft release has no binary. Ask Apollo to upload the built `.dmg` (from `cargo tauri build`) and publish the release — or remind him politely. A Show HN post with no downloadable file underperforms badly. If the .dmg is missing when HN conversation starts, that's okay-ish (build-from-source is documented), but the release MUST be live before any Product Hunt attempt.

---

## 6. KEY NOTES & PREFERENCES

- **Do not second-guess Apollo on stated facts.** If he says something, act on it.
- **Fire-and-forget style:** don't send progress updates mid-task; report on completion or real blockers.
- Apollo is a **university student in Iraq** (NTU). Keep the "solo dev in Iraq" story front-and-center in posts — it's the most compelling hook.
- The engine model for this chain is open-weight (DeepSeek-family). Follow the tool-call repair rules in `.cursorrules` (exact schema keys, no nulls, native arrays) — tool calls must be clean on first attempt.
- If you use the browser tools on GitHub, note that Pages deploys from `docs/` on `main` via workflow — a push to `docs/` triggers a rebuild automatically.
- If Apollo is unavailable and a post needs a decision, make the sensible default (post where traction is likely) and note it in your final report.

---

## 7. FINAL REPORT TO APOLLO — include

1. What you posted, where, when (URLs of posts)
2. Traction after 24–48h: stars, forks, comments, downloads
3. Common feedback themes + suggested next features (serious signals)
4. What still needs him: .dmg upload + publish release, engagement replies
5. Any payment/donation questions that came up

*End of handoff.*