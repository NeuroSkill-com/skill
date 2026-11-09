cask "neuroskill" do
  # version + sha256 are kept current by .github/workflows/homebrew-cask.yml,
  # which reconciles them against the latest stable GitHub release.
  version "0.1.1"
  sha256 "aec40c36f6a6f0b0106cc72c3f74e937e818e76b89b2984a3b030265d9232186"

  url "https://github.com/NeuroSkill-com/skill/releases/download/v#{version}/NeuroSkill_#{version}_aarch64.dmg"
  name "NeuroSkill"
  desc "State of Mind brain-computer interface system"
  homepage "https://github.com/NeuroSkill-com/skill"

  depends_on arch: :arm64
  # The bundled skill-daemon dynamically links Homebrew's libusb
  # (/opt/homebrew/opt/libusb/lib/libusb-1.0.0.dylib) and aborts on launch if
  # it is missing. Declaring it here makes `brew install` pull libusb in first,
  # so cask installs launch cleanly even while issue #55 is open.
  depends_on formula: "libusb"

  app "NeuroSkill.app"

  zap trash: [
    "~/Library/Application Support/com.neuroskill.skill",
    "~/Library/Caches/com.neuroskill.skill",
    "~/Library/Preferences/com.neuroskill.skill.plist",
    "~/Library/Saved Application State/com.neuroskill.skill.savedState"
  ]
end
