# Written by tools/release/package-managers.sh for each release, from the release's own signed
# checksums.
cask "steamgauge" do
  version "0.1.10"
  sha256 "9ce4e18ca4ec40af3e0bbd9024a5dca019d76d6a70e7909ce40abf246da23650"

  url "https://github.com/Aureliolo/steamgauge/releases/download/v#{version}/steamgauge-#{version}-macos-arm64.dmg"
  name "SteamGauge"
  desc "Finds out what players of a game think, from every Steam review"
  homepage "https://github.com/Aureliolo/steamgauge"

  livecheck do
    url :url
    strategy :github_latest
  end

  auto_updates true
  depends_on arch: :arm64
  depends_on macos: ">= :ventura"

  app "SteamGauge.app"
  shimscript = "#{staged_path}/steamgauge.wrapper.sh"
  binary shimscript, target: "steamgauge"

  preflight do
    File.write shimscript, <<~EOS
      #!/bin/sh
      exec '#{appdir}/SteamGauge.app/Contents/MacOS/steamgauge' "$@"
    EOS
  end

  zap trash: [
    "~/Library/Application Support/com.aureliolo.steamgauge",
    "~/Library/Caches/com.aureliolo.steamgauge",
    "~/Library/Caches/steamgauge",
    "~/Library/Preferences/com.aureliolo.steamgauge.plist",
    "~/Library/Saved Application State/com.aureliolo.steamgauge.savedState",
    "~/Library/WebKit/com.aureliolo.steamgauge",
  ]

  caveats <<~EOS
    SteamGauge is not notarised by Apple, so macOS warns the first time it opens.
    See https://github.com/Aureliolo/steamgauge#the-first-launch for what to do and why.
  EOS
end
