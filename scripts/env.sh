# Source this before building: `source scripts/env.sh`
# Toolchain locations on the dev Mac (see plan/13 M0 Spike Log.md).
export PATH="$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin:$PATH"   # rustup is keg-only in Homebrew
export JAVA_HOME="/Applications/Android Studio.app/Contents/jbr/Contents/Home"
export ANDROID_HOME="$HOME/Library/Android/sdk"
export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$ANDROID_HOME/cmdline-tools/latest/bin:$PATH"
