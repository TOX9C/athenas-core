# Copy this file into the TOX9C/homebrew-tap repo at Casks/athenas-core.rb.
# version + sha256 are placeholders: fill from the GitHub release at publish
# time (release-macos.yml already uploads "<dmg>.sha256" alongside the DMG).
cask "athenas-core" do
  version "3.3.0"
  sha256 "REPLACE_WITH_RELEASE_SHA256"

  url "https://github.com/TOX9C/athenas-core/releases/download/v#{version}/Athena%27s%20Core_#{version}_aarch64.dmg"
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
