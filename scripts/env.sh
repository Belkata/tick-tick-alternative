# Source this before building: `source scripts/env.sh`
# Toolchain locations on the dev Mac and the Arch laptop (see plan/13 M0 Spike Log.md).
export PATH="$HOME/.cargo/bin:$PATH"

case "$(uname -s)" in
  Darwin)
    export PATH="/opt/homebrew/opt/rustup/bin:$PATH"   # rustup is keg-only in Homebrew
    export PATH="/opt/homebrew/bin:$PATH"              # Homebrew python3 (3.14): the Android QA plugin scripts need 3.10+
    export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
    export ANDROID_HOME="$HOME/Library/Android/sdk"
    ;;
  Linux)
    # Arch: Android Studio (AUR `android-studio`) puts the SDK in ~/Android/Sdk and ships its own JBR.
    for jbr in /opt/android-studio/jbr "$HOME/android-studio/jbr" /usr/lib/jvm/default; do
      if [ -d "$jbr" ]; then export JAVA_HOME="$jbr"; break; fi
    done
    export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
    # Flutter is a git checkout of the release tag (git clone -b 3.50.1 https://github.com/flutter/flutter.git ~/flutter).
    if [ -d "$HOME/flutter/bin" ]; then export PATH="$HOME/flutter/bin:$PATH"; fi
    ;;
esac

export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$ANDROID_HOME/cmdline-tools/latest/bin:$PATH"
