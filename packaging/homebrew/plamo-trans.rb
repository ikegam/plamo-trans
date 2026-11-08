# Homebrew formula for ikegam/homebrew-tap (installs the prebuilt release binaries).
# Version and checksums are filled in by packaging/update.sh on release.
class PlamoTrans < Formula
  desc "Local translation as a Unix filter: PLaMo 3 Translate on llama.cpp, no server"
  homepage "https://github.com/ikegam/plamo-trans"
  version "0.1.0"
  license all_of: ["BSD-4-Clause", "MIT"]

  on_macos do
    depends_on arch: :arm64
    url "https://github.com/ikegam/plamo-trans/releases/download/v#{version}/plamo-trans-#{version}-macos-arm64.tar.gz"
    sha256 "0000000000000000000000000000000000000000000000000000000000000000" # macos-arm64
  end

  on_linux do
    on_intel do
      url "https://github.com/ikegam/plamo-trans/releases/download/v#{version}/plamo-trans-#{version}-linux-x86_64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000" # linux-x86_64
    end
    on_arm do
      url "https://github.com/ikegam/plamo-trans/releases/download/v#{version}/plamo-trans-#{version}-linux-arm64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000" # linux-arm64
    end
  end

  conflicts_with "translate-shell", because: "both install a `trans` executable"

  def install
    bin.install "trans"
    doc.install "README.md"
  end

  def caveats
    <<~EOS
      The model is not included. Read the PLaMo Community License, then download one:
        trans --download 8b      # 5.3 GB, the default
        trans --download 0.8b    # 0.6 GB, for -ff
      https://huggingface.co/pfnet/plamo-3-translate-31b-GGUF
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/trans -V")
  end
end
