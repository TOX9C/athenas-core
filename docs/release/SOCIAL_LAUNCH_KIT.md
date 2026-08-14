# Athena's Core social launch kit

Use the redesigned landing page as the destination for every post:

- Landing page: `docs/index.html`
- Product film: embedded on the landing page under **Film**
- Source and releases: `https://github.com/TOX9C/athenas-core`

The copy below describes Athena's Core as an early-access macOS beta. Keep that wording until the release scope changes.

## X / Twitter launch post

> We built Athena's Core: a native macOS workspace for AI-assisted development.
>
> Terminal panes, workspace-aware chat, tasks, and agent coordination — together in one window.
>
> It is early, it is real, and we want people to try it.
>
> Watch the 25-second film + join the beta: [LANDING_PAGE_URL]

## X / Twitter thread

**Post 1**

> Most coding sessions do not need more tabs. They need less context switching.
>
> Athena's Core puts the terminal, AI chat, tasks, and agents in one native workspace.

**Post 2**

> The terminal stays close. Athena can see the workspace you are already working in. The task board shows what happens next.
>
> This is the direction: one focused desktop window instead of a pile of disconnected tools.

**Post 3**

> Athena's Core is an early macOS beta for Apple silicon.
>
> Try it, tell us what breaks, and help shape what comes next: [RELEASES_URL]

## Short-form video caption

> One window for the work. Athena's Core is an early macOS beta for AI-assisted development — terminal, chat, tasks, and agents in one native workspace. Try it and tell us what you think. [LANDING_PAGE_URL]

## Reddit / developer community post

**Title:** We built a native macOS workspace for AI-assisted development — looking for early testers

**Body:**

> Athena's Core is a desktop workspace that combines multiple terminal panes, workspace-aware AI chat, a Kanban task board, and multi-agent coordination in one window.
>
> It is built with Rust + Tauri 2 and is currently an early macOS beta for Apple silicon. We made a short product film using real captures from the app and put together a landing page with the current workflow and screenshots.
>
> We are looking for developers who are willing to try the current build and give direct feedback on what is useful, confusing, or missing.
>
> Landing page: [LANDING_PAGE_URL]
> Releases: [RELEASES_URL]
> Feedback: [ISSUES_URL]

## Publishing checklist

- [ ] Replace `[LANDING_PAGE_URL]` with the deployed site URL.
- [ ] Confirm the linked release is the intended macOS Apple silicon build.
- [ ] Test the embedded film on desktop and mobile before posting.
- [ ] Add UTM parameters per channel, for example `?utm_source=x&utm_medium=social&utm_campaign=beta_launch`.
- [ ] Pin the launch post on X / Twitter.
- [ ] Reply to early testers with the feedback issue link.
- [ ] Do not claim downloads, performance numbers, platforms, or features that are not in the current release.
- [ ] Review incoming issues daily during the first week.
