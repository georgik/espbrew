class Espbrew < Formula
  desc "ESP32 Multi-Board Build Manager with TUI and CLI interfaces"
  homepage "https://github.com/georgik/espbrew"
  version "0.0.2"
  url "https://github.com/georgik/espbrew/releases/download/v#{version}/espbrew-macos-arm64.tar.gz"
  sha256 "9ae35473930d28ef106314e5050441cdb8fe0bde26fcff342043f57a40641005"
  license "MIT"

  # Only support Apple Silicon Macs (ARM64)
  depends_on arch: :arm64

  def install
    bin.install "espbrew"
    
    # Install documentation if available
    doc.install "README.md" if File.exist?("README.md")
  end

  test do
    # Test that the binary works and shows version
    system "#{bin}/espbrew", "--version"
  end

  def caveats
    <<~EOS
      espbrew is a TUI and CLI tool for managing ESP-IDF builds across multiple board configurations.
      
      Usage:
        espbrew                    # Interactive TUI mode
        espbrew --cli-only         # CLI-only mode for automation
        espbrew /path/to/project   # Specify project directory
      
      For more information, visit: https://github.com/georgik/espbrew
    EOS
  end
end