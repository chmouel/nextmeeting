#!/usr/bin/env bash
# Author: Chmouel Boudjnah <chmouel@chmouel.com>
set -euxfo pipefail

reset() {
  docker rm -v -f radicale >/dev/null 2>&1 || true
}
trap "reset" EXIT

reset
docker run -d --name radicale -p5232:5232 -v $PWD/passwd:/etc/radicale/passwd -v $PWD/config.ini:/etc/radicale/config -it ghcr.io/kozea/radicale:latest
docker exec -it radicale mkdir -p /var/lib/radicale/collections/collection-root/username/calendar
docker exec -it radicale sh -c 'echo "{\"tag\": \"VCALENDAR\"}" > /var/lib/radicale/collections/collection-root/username/calendar/.Radicale.props'
sleep 2
uv run ./populate.py

uv run nextmeeting --caldav-url http://localhost:5232/username/calendar --caldav-username username --caldav-password password --today-only

# put server in foreground
docker logs -f radicale
