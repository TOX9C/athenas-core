cask "athenas-core" do
  version "3.4.1"
  sha256 "1766dc639016248d6bafc800f59b81f11f7b95aadcfeab2fab5aa4c205737e0b"

  # NOTE: the local DMG is "Athena's Core_<v>_aarch64.dmg", but GitHub
  # normalizes the asset name to dots on upload. On each release, bump
  # version + sha256 here from the release's .sha256 asset and confirm
  # the dot-form asset name still matches the release page.
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
