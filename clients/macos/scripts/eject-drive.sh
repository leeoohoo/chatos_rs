#!/bin/bash

# Safely eject a removable disk on macOS. If files are busy, show the
# processes using the disk, ask before terminating them, and retry.

set -u

SCRIPT_NAME=$(basename "$0")
AUTO_YES=0
ALLOW_FORCE=0
WAIT_SECONDS=8

usage() {
  cat <<EOF
Usage:
  $SCRIPT_NAME [--yes] [--force] [--wait SECONDS] <volume name|mount path|disk id>

Examples:
  $SCRIPT_NAME "My SSD"
  $SCRIPT_NAME "/Volumes/My SSD"
  $SCRIPT_NAME disk4
  $SCRIPT_NAME --yes "My SSD"
  $SCRIPT_NAME --yes --force disk4

Options:
  -y, --yes       Do not ask before sending SIGTERM to user processes.
  -f, --force     If SIGTERM is not enough, allow SIGKILL and forced unmount.
  -w, --wait N    Wait N seconds after SIGTERM (default: $WAIT_SECONDS).
  -h, --help      Show this help.

Warning: --force can cause unsaved data to be lost. The script does not
terminate system/root processes or its own parent shell.
EOF
}

die() {
  echo "Error: $*" >&2
  exit 1
}

command -v diskutil >/dev/null 2>&1 || die "This script requires macOS diskutil."
command -v lsof >/dev/null 2>&1 || die "lsof was not found."

physical_disk_for() {
  item_info=$(diskutil info "$1" 2>/dev/null) || return 1
  item_id=$(printf '%s\n' "$item_info" | awk -F: '/^[[:space:]]*Device Identifier:/ {sub(/^[[:space:]]+/, "", $2); print $2; exit}')
  item_whole=$(printf '%s\n' "$item_info" | awk -F: '/^[[:space:]]*Part of Whole:/ {sub(/^[[:space:]]+/, "", $2); print $2; exit}')
  item_store=$(printf '%s\n' "$item_info" | awk -F: '/^[[:space:]]*APFS Physical Store:/ {sub(/^[[:space:]]+/, "", $2); print $2; exit}')

  [ -n "$item_whole" ] || item_whole=$item_id
  if [ -n "$item_store" ]; then
    store_info=$(diskutil info "$item_store" 2>/dev/null) || return 1
    store_whole=$(printf '%s\n' "$store_info" | awk -F: '/^[[:space:]]*Part of Whole:/ {sub(/^[[:space:]]+/, "", $2); print $2; exit}')
    [ -n "$store_whole" ] && item_whole=$store_whole
  fi

  [ -n "$item_whole" ] || return 1
  printf '%s\n' "$item_whole"
}

TARGET=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -y|--yes)
      AUTO_YES=1
      shift
      ;;
    -f|--force)
      ALLOW_FORCE=1
      shift
      ;;
    -w|--wait)
      [ "$#" -ge 2 ] || die "--wait needs a number of seconds."
      WAIT_SECONDS=$2
      case "$WAIT_SECONDS" in
        ''|*[!0-9]*) die "--wait must be a non-negative integer." ;;
      esac
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      [ "$#" -gt 0 ] || die "Missing disk or volume."
      TARGET=$1
      shift
      [ "$#" -eq 0 ] || die "Only one disk or volume can be ejected at a time."
      ;;
    -*)
      die "Unknown option: $1"
      ;;
    *)
      [ -z "$TARGET" ] || die "Only one disk or volume can be ejected at a time."
      TARGET=$1
      shift
      ;;
  esac
done

[ -n "$TARGET" ] || {
  usage >&2
  exit 2
}

if [ -d "/Volumes/$TARGET" ]; then
  TARGET="/Volumes/$TARGET"
fi

DISK_INFO=$(diskutil info "$TARGET" 2>/dev/null) || \
  die "Cannot find '$TARGET'. Try a path such as /Volumes/My SSD or an id such as disk4."

DEVICE_ID=$(printf '%s\n' "$DISK_INFO" | awk -F: '/^[[:space:]]*Device Identifier:/ {sub(/^[[:space:]]+/, "", $2); print $2; exit}')
WHOLE_DISK=$(printf '%s\n' "$DISK_INFO" | awk -F: '/^[[:space:]]*Part of Whole:/ {sub(/^[[:space:]]+/, "", $2); print $2; exit}')

[ -n "$DEVICE_ID" ] || die "Could not determine the device identifier."
WHOLE_DISK=$(physical_disk_for "$TARGET") || \
  die "Could not resolve '$TARGET' to a physical disk."

WHOLE_INFO=$(diskutil info "$WHOLE_DISK" 2>/dev/null) || \
  die "Could not inspect physical disk /dev/$WHOLE_DISK."
INTERNAL=$(printf '%s\n' "$WHOLE_INFO" | awk -F: '/^[[:space:]]*Internal:/ {sub(/^[[:space:]]+/, "", $2); print $2; exit}')
LOCATION=$(printf '%s\n' "$WHOLE_INFO" | awk -F: '/^[[:space:]]*Device Location:/ {sub(/^[[:space:]]+/, "", $2); print $2; exit}')

if [ "$INTERNAL" = "Yes" ] || [ "$LOCATION" = "Internal" ]; then
  die "Refusing to operate on internal disk /dev/$WHOLE_DISK."
fi

# The script itself must not keep the target volume busy.
cd / || exit 1

echo "Target: /dev/$WHOLE_DISK"
echo "Trying a normal unmount first..."
if diskutil unmountDisk "$WHOLE_DISK"; then
  echo "Unmounted. Ejecting..."
  diskutil eject "$WHOLE_DISK" || die "Unmounted, but diskutil could not eject /dev/$WHOLE_DISK."
  echo "Done. You can safely disconnect the disk."
  exit 0
fi

# Find every mounted volume backed by this physical disk. This also includes
# APFS volumes whose visible device id is a synthesized disk (for example
# disk5s1 backed by physical disk4s3).
MOUNT_POINTS=$(
  for mount_point in /Volumes/*; do
    [ -d "$mount_point" ] || continue
    candidate_disk=$(physical_disk_for "$mount_point" 2>/dev/null) || continue
    if [ "$candidate_disk" = "$WHOLE_DISK" ]; then
      printf '%s\n' "$mount_point"
    fi
  done | sort -u
)

[ -n "$MOUNT_POINTS" ] || die "No mounted volumes were found for /dev/$WHOLE_DISK."

echo
echo "The disk is busy. Looking for processes using it (this may take a moment)..."

PID_FILE=$(mktemp -t eject-drive-pids.XXXXXX) || die "Could not create a temporary file."
cleanup() {
  rm -f "$PID_FILE"
}
trap cleanup EXIT INT TERM

printf '%s\n' "$MOUNT_POINTS" | while IFS= read -r mount_point; do
  [ -n "$mount_point" ] || continue
  # +f selects the complete mounted filesystem without recursively walking
  # every directory, which is much faster on large drives.
  lsof -nP -t +f -- "$mount_point" 2>/dev/null || true
done | sort -u > "$PID_FILE"

# Protect this script, its parent shells, launchd, and root-owned services.
PROTECTED_PIDS=" $$ "
ancestor=$PPID
while [ "$ancestor" -gt 1 ] 2>/dev/null; do
  PROTECTED_PIDS="$PROTECTED_PIDS$ancestor "
  ancestor=$(ps -p "$ancestor" -o ppid= 2>/dev/null | tr -d ' ')
  [ -n "$ancestor" ] || break
done

FILTERED_FILE="${PID_FILE}.filtered"
: > "$FILTERED_FILE"
while IFS= read -r pid; do
  [ -n "$pid" ] || continue
  case "$PROTECTED_PIDS" in
    *" $pid "*) continue ;;
  esac
  owner=$(ps -p "$pid" -o user= 2>/dev/null | awk '{print $1}')
  [ -n "$owner" ] || continue
  [ "$owner" != "root" ] || continue
  printf '%s\n' "$pid" >> "$FILTERED_FILE"
done < "$PID_FILE"
mv "$FILTERED_FILE" "$PID_FILE"

if [ ! -s "$PID_FILE" ]; then
  echo "No terminable user process was found. A system service or a shell's current directory may be blocking the disk."
  echo "Close Finder windows for this disk, leave its directory in Terminal (run: cd /), and try again."
  if [ "$ALLOW_FORCE" -eq 1 ]; then
    echo "Trying a forced unmount because --force was supplied..."
    diskutil unmountDisk force "$WHOLE_DISK" || die "Forced unmount failed."
    diskutil eject "$WHOLE_DISK" || die "Unmounted, but eject failed."
    echo "Done. You can safely disconnect the disk."
    exit 0
  fi
  exit 1
fi

echo
printf '%-8s %-16s %s\n' "PID" "USER" "PROGRAM"
while IFS= read -r pid; do
  user=$(ps -p "$pid" -o user= 2>/dev/null | awk '{print $1}')
  command=$(ps -p "$pid" -o command= 2>/dev/null)
  printf '%-8s %-16s %s\n' "$pid" "$user" "$command"
done < "$PID_FILE"

if [ "$AUTO_YES" -ne 1 ]; then
  echo
  printf 'Ask these programs to quit? Unsaved work may be lost. [y/N] '
  read -r answer
  case "$answer" in
    y|Y|yes|YES) ;;
    *) die "Cancelled; nothing was terminated." ;;
  esac
fi

echo "Sending a normal termination request..."
while IFS= read -r pid; do
  kill -TERM "$pid" 2>/dev/null || true
done < "$PID_FILE"

elapsed=0
while [ "$elapsed" -lt "$WAIT_SECONDS" ]; do
  still_running=0
  while IFS= read -r pid; do
    if kill -0 "$pid" 2>/dev/null; then
      still_running=1
      break
    fi
  done < "$PID_FILE"
  [ "$still_running" -eq 1 ] || break
  sleep 1
  elapsed=$((elapsed + 1))
done

echo "Retrying unmount..."
if diskutil unmountDisk "$WHOLE_DISK"; then
  diskutil eject "$WHOLE_DISK" || die "Unmounted, but eject failed."
  echo "Done. You can safely disconnect the disk."
  exit 0
fi

if [ "$ALLOW_FORCE" -ne 1 ]; then
  die "The disk is still busy. Review the programs above, or retry with --force if losing unsaved data is acceptable."
fi

echo "The disk is still busy; --force was supplied. Force-stopping remaining user processes..."
while IFS= read -r pid; do
  kill -KILL "$pid" 2>/dev/null || true
done < "$PID_FILE"

diskutil unmountDisk force "$WHOLE_DISK" || die "Forced unmount failed."
diskutil eject "$WHOLE_DISK" || die "Unmounted, but eject failed."
echo "Done. You can safely disconnect the disk."
