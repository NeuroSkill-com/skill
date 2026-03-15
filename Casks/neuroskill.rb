cask "neuroskill" do
  version "0.0.37"
  sha256 "84ed44e8e278805815d408c3da9886a529f24faceab1cf35309ab450dfca949a"

  url "https://github.com/NeuroSkill-com/skill/releases/download/v#{version}/NeuroSkill_#{version}_aarch64.dmg"
  name "NeuroSkill"
  desc "State of Mind brain-computer interface system"
  homepage "https://github.com/NeuroSkill-com/skill"

  depends_on arch: :arm64

  app "NeuroSkill.app"

  zap trash: [
    "~/Library/Application Support/com.neuroskill.skill",
    "~/Library/Caches/com.neuroskill.skill",
    "~/Library/Preferences/com.neuroskill.skill.plist",
    "~/Library/Saved Application State/com.neuroskill.skill.savedState"
  ]
end
