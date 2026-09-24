# Copy this file into the TOX9C/homebrew-tap repo at Casks/athenas-core.rb.
# version + sha256 are placeholders: fill from the GitHub release at publish
# time (release-macos.yml already uploads "<dmg>.sha256" alongside the DMG).
cask "athenas-core" do
  version "3.3.0"
  sha256 "4a7192b44917934e125ec19002c8f4d01cf239d3be66b0dd4f60329380dbfd31"

  # NOTE: v3.3.0's uploaded asset is "Athena.s.Core_3.3.0_aarch64.dmg"
  # (space+apostrophe normalized to dots on upload), but the release
  # workflow's --expected-name gate expects "Athena's Core_<v>_aarch64.dmg".
  # One of them is wrong — re-check the actual asset name when the next
  # tag publishes and align the URL below (or the workflow gate).
  url "https://github.com/TOX9C/athenas-core/releases/download/v#{version}/Athena.s.Core_#{version}_aarch64.dmg"
  name "Athena's Core"
  desc "Native macOS workspace: terminal, AI chat, task board, and agent team"
  homepage "https://github.com/TOX9C/athenas-core"

  depends_on macos: ">= :ventura"
  depends_on arch: :arm64

  app "Athena's Core.app"

  zap trash: [
    "~/Library/Application Support/com.athena.core",
    "~/Library/Preferences/com.athena.core.plist",
  ]
end
