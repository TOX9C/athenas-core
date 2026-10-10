# Copy this file into the TOX9C/homebrew-tap repo at Casks/athenas-core.rb.
# version + sha256 are placeholders: fill from the GitHub release at publish
# time (release-macos.yml already uploads "<dmg>.sha256" alongside the DMG).
cask "athenas-core" do
  version "3.4.0"
  sha256 "61419832ddced95a15bee07773b9767133fec2e6875d1575e47d0a5b131a545c"

  # NOTE: the local DMG is "Athena's Core_<v>_aarch64.dmg" (passes the
  # release workflow's --expected-name gate against the local artifact),
  # but GitHub normalizes the asset name to dots on upload. Do NOT change
  # the workflow gate — on each release, update version+sha256 here and
  # confirm the dot-form asset name still matches the release page.
  url "https://github.com/TOX9C/athenas-core/releases/download/v#{version}/Athena.s.Core_#{version}_aarch64.dmg"
  name "Athena's Core"
  desc "Native macOS workspace: terminal, AI chat, task board, and agent team"
  homepage "https://github.com/TOX9C/athenas-core"

  depends_on macos: :ventura
  depends_on arch: :arm64

  app "Athena's Core.app"

  zap trash: [
    "~/Library/Application Support/com.athena.core",
    "~/Library/Preferences/com.athena.core.plist",
  ]
end
