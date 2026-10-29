#!/bin/sh
# One load-test run against the server in a docker container limited like a VPS (default: OVH
# VPS-1, 2 vCores and 4 GB), with the load generator in the same Linux VM (colima), outside the
# container: tools/vps_run.sh <name> [loadtest args...]
# Build the image (server + loadtest) in the VM first, e.g.:
#   tar cf - Cargo.toml Cargo.lock engine rules goldens arena-server loadtest protocol/data \
#     tools/Dockerfile.perf | colima ssh -- docker build -t arena-perf -f tools/Dockerfile.perf -
# Result JSON in perf/<name>.json. VPS_CPUS / VPS_MEMORY override.
set -e
cd "$(dirname "$0")/.."
NAME=$1; shift
OUT="$(pwd)/perf/$NAME.json"
cd /
colima ssh -- sh -c "
docker rm -f arena-perf > /dev/null 2>&1 || true
[ -x /tmp/loadtest ] || { docker run --rm --entrypoint cat arena-perf /app/loadtest > /tmp/loadtest && chmod +x /tmp/loadtest; }
docker run -d --name arena-perf --cpus=${VPS_CPUS:-2} --memory=${VPS_MEMORY:-4g} -p 8097:8080 -e PERSIST_SECS=${PERSIST_SECS:-10} arena-perf > /dev/null
sleep 2
ulimit -n 65536 2>/dev/null || ulimit -n 4096; /tmp/loadtest --url http://localhost:8097 --docker arena-perf $* > /tmp/vps_result.json 2> /tmp/vps_progress.txt
docker exec arena-perf sh -c 'du -sh /data/games; ls /data/games | wc -l' >&2
docker inspect -f 'state {{.State.Status}} oom {{.State.OOMKilled}} exit {{.State.ExitCode}}' arena-perf >&2
docker logs arena-perf 2>&1 | grep -iE 'panic|overflow|killed|memory' | tail -3 >&2 || true
docker rm -f arena-perf > /dev/null
cat /tmp/vps_result.json
" > "$OUT"
python3 -c "
import json; d = json.load(open('$OUT'))
print({k: round(d[k], 3) if isinstance(d[k], float) else d[k] for k in ['games', 'thinkMs', 'answers', 'answersPerSec', 'polls', 'gamesFinished', 'errors', 'refused', 'serverCpuMsPerAnswer', 'serverCpuPercentAvg', 'serverMemCurrentMBMax', 'serverTxBytesPerRequest', 'serverTxMbitPerSec']}, d['answerLatencyMs'], 'steady', d.get('answerLatencySteadyMs'), 'create', d['createLatencyMs'], 'mem', d['serverRssMB'])"
