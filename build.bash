#!/usr/bin/env bash
set -euo pipefail

# Builds one lecture (or all of them) with Cargo, and optionally runs it.

# Cargo finds the workspace from here.
cd "$(dirname "$0")"

# Lecture

# Every lecture_N/ with a Cargo.toml, in numeric order (sort -V puts lecture_10 after lecture_9).
mapfile -t lectures < <(
  for manifest in lecture_*/Cargo.toml; do
    echo "${manifest%%/*}"
  done | sort -V
)

echo
echo "0) All lectures"
for i in "${!lectures[@]}"; do
  echo "$((i + 1))) ${lectures[$i]}"
done
echo

read -rp "Choose a lecture [0-${#lectures[@]}]: " lecture_choice

if [[ "$lecture_choice" == 0 ]]; then
  TARGET="all"
elif [[ "$lecture_choice" =~ ^[0-9]+$ ]] && (( lecture_choice >= 1 && lecture_choice <= ${#lectures[@]} )); then
  TARGET="${lectures[$((lecture_choice - 1))]}"
else
  echo "Invalid choice"
  exit 1
fi

# Configuration

# Running needs a single lecture, so "All lectures" only offers the builds.
if [[ "$TARGET" == all ]]; then
  echo "
Build
 1) Debug
 2) Release
 3) Clippy (lints, no executable)
"
  read -rp "Choose [1-3, Enter for 1]: " choice
  choice="${choice:-1}"
  max=3
else
  echo "
Build                            Build & run
 1) Debug                         4) Debug
 2) Release                       5) Release
 3) Clippy (lints, no executable)
"
  read -rp "Choose [1-5, Enter for 4]: " choice
  choice="${choice:-4}"
  max=5
fi

if ! [[ "$choice" =~ ^[0-9]+$ ]] || (( choice < 1 || choice > max )); then
  echo "Invalid choice: $choice" >&2
  exit 1
fi

# 4 and 5 are 1 and 2 plus a run afterwards.
run=false
if (( choice > 3 )); then
  run=true
  choice=$(( choice - 3 ))
fi

# -p picks one package of the workspace; --workspace takes them all.
if [[ "$TARGET" == all ]]; then
  packages=(--workspace)
else
  packages=(-p "$TARGET")
fi

# Build

case "$choice" in
  1) cargo build "${packages[@]}";           PROFILE="debug" ;;
  2) cargo build --release "${packages[@]}"; PROFILE="release" ;;
  3) cargo clippy "${packages[@]}";          exit 0 ;;
esac

if [[ "$run" == false ]]; then
  exit 0
fi

# Run

echo
"./target/$PROFILE/$TARGET"
