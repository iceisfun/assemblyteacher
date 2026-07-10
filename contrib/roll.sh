#!/bin/bash
set -euo pipefail

# Roll Assembly Teacher to docker5: build + push the image, redeploy the
# container, and smoke-test it.
#
#   ./contrib/roll.sh
#
# Follows the same shape as ~/work/iceisfun_com/contrib/{build,publish}, but
# self-contained: the pull/replace/run happens inline over SSH rather than via a
# create-* script installed on the host.
#
# Routing (already configured, external to this script):
#   eax.iceisfun.com  --DNS(CNAME @)-->  the edge proxy
#                     --proxy/TLS-->     10.2.162.18:54323  (docker5)
# So this script only has to make the container answer on docker5:54323.

# ---- configuration ---------------------------------------------------------
NAME="assemblyteacher"                                   # image + container name
REPO="docker-repo.dev.moonlightcompanies.com"
IMAGE="$REPO/$NAME"
HOST="it@docker5.dev.moonlightcompanies.com"
PORT="54323"                                             # host port on docker5
PUBLIC_URL="https://eax.iceisfun.com/api/health"         # best-effort end-to-end check

# ---- output ----------------------------------------------------------------
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
log()   { echo -e "${GREEN}[$(date +'%Y-%m-%d %H:%M:%S')] $1${NC}"; }
warn()  { echo -e "${YELLOW}[$(date +'%Y-%m-%d %H:%M:%S')] WARNING: $1${NC}"; }
error() { echo -e "${RED}[$(date +'%Y-%m-%d %H:%M:%S')] ERROR: $1${NC}" >&2; exit 1; }

# Ctrl-C aborts the whole script cleanly, not just the current child.
trap 'echo; warn "Interrupted by user."; exit 130' INT

cd "$(dirname "$0")/.." || error "Failed to change to project directory"

SSH=(ssh -o ConnectTimeout=15 -o StrictHostKeyChecking=accept-new "$HOST")

# ---- 0. sanity -------------------------------------------------------------
for f in Cargo.toml crates/server/src/main.rs web/package.json contrib/Dockerfile.deploy; do
    [ -e "$f" ] || error "Required path not found: $f (run from a clean checkout)"
done
[ -d lessons ] || error "lessons/ not found"
command -v docker >/dev/null || error "docker not found on this machine"

# ---- 1. build + push -------------------------------------------------------
log "Building image $IMAGE"
DOCKER_BUILDKIT=1 docker build \
    --network=host \
    -t "$IMAGE" \
    -f contrib/Dockerfile.deploy . \
    || error "Docker build failed"

log "Pushing $IMAGE"
docker push "$IMAGE" \
    || error "Docker push failed (are you logged in to $REPO?)"

log "Image: $(docker image inspect "$IMAGE" --format '{{.Id}} {{.Size}}')"

# ---- 2. roll on docker5 ----------------------------------------------------
# Pull the new image, drop the old container, run the new one on $PORT. The
# container needs no writable filesystem, no capabilities and no privilege
# escalation, so it is locked down accordingly — it is a public service that
# emulates submitted code, and defence in depth is cheap.
log "Rolling container on $HOST (port $PORT)"
"${SSH[@]}" bash -s -- "$IMAGE" "$NAME" "$PORT" <<'REMOTE' || error "Remote roll failed"
    set -euo pipefail
    IMAGE="$1"; NAME="$2"; PORT="$3"
    docker pull "$IMAGE"
    docker rm -f "$NAME" >/dev/null 2>&1 || true
    docker run -d \
        --name "$NAME" \
        --restart unless-stopped \
        --read-only --tmpfs /tmp \
        --cap-drop ALL \
        --security-opt no-new-privileges \
        -p "${PORT}:${PORT}" \
        "$IMAGE" >/dev/null
    echo "container started"
REMOTE

# ---- 3. wait for it to come up ---------------------------------------------
log "Waiting for the container to report Up..."
status=""
for _ in $(seq 1 30); do
    status=$("${SSH[@]}" "docker ps --filter name=^/${NAME}\$ --format '{{.Status}}'" 2>/dev/null || true)
    case "$status" in
        *unhealthy*) error "Container reports unhealthy" ;;
        *Up*)        break ;;
    esac
    sleep 2
done
case "$status" in
    *Up*) log "  ${NAME}: ${status}" ;;
    *)    error "Container ${NAME} did not come up (try: ssh $HOST docker logs $NAME)" ;;
esac

# ---- 4. smoke test ---------------------------------------------------------
# The authoritative check runs on the host against the published port: it proves
# the container is actually serving, independent of DNS or the edge proxy.
log "Smoke test on host: localhost:${PORT}/api/health"
health=""
for _ in $(seq 1 15); do
    health=$("${SSH[@]}" "curl -fsS -m 5 http://localhost:${PORT}/api/health" 2>/dev/null || true)
    [ -n "$health" ] && break
    sleep 2
done
case "$health" in
    *'"status":"ok"'*) log "  host health: $health" ;;
    *)                 error "Host health check failed (got: '${health:-no response}')" ;;
esac

# The end-to-end check goes through DNS + the edge proxy. Best-effort: routing
# may still be propagating, and that is outside this script's control.
log "End-to-end check: $PUBLIC_URL"
code=$(curl -s -m 15 -o /dev/null -w '%{http_code}' "$PUBLIC_URL" 2>/dev/null || echo 000)
if [ "$code" = "200" ]; then
    log "  $PUBLIC_URL -> $code"
else
    warn "  $PUBLIC_URL -> $code (host check passed; edge routing/DNS may still be settling)"
fi

log "Rolled $NAME successfully."
