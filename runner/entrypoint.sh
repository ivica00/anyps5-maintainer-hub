#!/bin/bash
set -e

REPO="${REPO:-boykopovar/AnyPS5}"
RUNNER_NAME="${RUNNER_NAME:-$(hostname)-docker-worker}"
LABELS="${LABELS:-linux,x64,anyps5-builder,docker}"

if [ -z "$RUNNER_TOKEN" ]; then
    echo "[-] ERROR: RUNNER_TOKEN environment variable is required."
    echo "    Obtain a token from: https://github.com/$REPO/settings/actions/runners/new"
    exit 1
fi

echo "[+] Configuring AnyPS5 Self-Hosted Runner for $REPO..."
./config.sh \
    --url "https://github.com/$REPO" \
    --token "$RUNNER_TOKEN" \
    --name "$RUNNER_NAME" \
    --labels "$LABELS" \
    --work "_work" \
    --unattended \
    --replace \
    ${EPHEMERAL:+--ephemeral}

cleanup() {
    echo "[!] Removing runner registration..."
    ./config.sh remove --token "$RUNNER_TOKEN" || true
}
trap 'cleanup; exit 130' INT
trap 'cleanup; exit 143' TERM

echo "[+] Starting runner..."
./run.sh "$@" &
wait $!
