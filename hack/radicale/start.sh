#!/usr/bin/env bash
# Author: Chmouel Boudjnah <chmouel@chmouel.com>
set -euxfo pipefail

reset() {
  docker rm -v -f radicale >/dev/null 2>&1 || true
}

wait_for_radicale() {
    local retries=30
    local wait=1
    while ! curl -s "http://localhost:5232" >/dev/null; do
        retries=$((retries - 1))
        if [ $retries -le 0 ]; then
            echo "Timeout waiting for Radicale to start"
            exit 1
        fi
        sleep $wait
    done
}

trap "reset" EXIT

reset
docker run -d --name radicale -p5232:5232 -v $PWD/passwd:/etc/radicale/passwd -v $PWD/config.ini:/etc/radicale/config ghcr.io/kozea/radicale:latest
docker exec radicale mkdir -p /var/lib/radicale/collections/collection-root/username/calendar
docker exec radicale sh -c 'echo "{\"tag\": \"VCALENDAR\"}" > /var/lib/radicale/collections/collection-root/username/calendar/.Radicale.props'
wait_for_radicale
uv run ./populate.py

uv run nextmeeting --caldav-url http://localhost:5232/username/calendar --caldav-username username --caldav-password password --today-only

# put server in foreground
docker logs -f radicale
