#!/usr/bin/env bash
set -euo pipefail -o posix

log() {
  echo
  echo '----------'
  echo "[$(date --iso-8601=seconds)] $1"
  echo '----------'
  echo
}

opencode_run() {
  local mode="$1"
  log "Now execute ${mode}"
  opencode run "$(cat "${DIR}/${mode}.md")"
}

iterations=0
DIR="$(dirname "${BASH_SOURCE[0]}")"
prev_state=''
same_count=0
FALLBACK_THRESHOLD=30

while true; do
  state=$(head -n 1 "${DIR}/../tmp/STATE.md" 2>/dev/null | tr -d '\n')
  if [ "$state" = "$prev_state" ]; then
    same_count=$((same_count + 1))
  else
    same_count=1
  fi
  if [ "$same_count" -ge "$FALLBACK_THRESHOLD" ]; then
    log "Contiguous '$state' for ${same_count} times; exiting."
    exit 1
  fi
  prev_state="$state"

  log "Start of an execution (iterations=${iterations} state=${state})."
  case "$state" in
    "IDLE")             opencode_run design ;;
    "DESIGNED")         opencode_run implement ;;
    "CHANGE_REQUESTED") opencode_run implement ;;
    "IMPLEMENTED")      opencode_run review ;;
    "REVIEW_PASSED")    opencode_run qa ;;
    "QA_PASSED")        opencode_run sweep ;;
    "NO_TASK")
      log "No task found; quitting."
      exit 0
      ;;
    *)
      log "unknown state: ${state}. stopping"
      exit 1
      ;;
  esac
  log "End of an execution (iterations=${iterations} state=${state}). Sleep for a while and continue..."
  sleep 10
  ((++iterations))
done
