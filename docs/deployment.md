# Deployment

Assembly Teacher is designed to sit behind a TLS-terminating reverse proxy. The
application speaks plain HTTP, serves both the API and the built frontend from a
single origin, and never reads a certificate.

```
   Internet ──TLS──▶  reverse proxy  ──HTTP──▶  asmteacher :8080
                      (nginx/Caddy)             (API + web/dist)
                      rate limiting
                      connection caps
```

## Running

```sh
contrib/build.sh    # produces target/release/asmteacher and web/dist

target/release/asmteacher \
    --listen 127.0.0.1:8080 \
    --web    web/dist \
    --lessons lessons
```

Bind to loopback and let the proxy reach it there. Flags also read from the
environment: `ASMTEACHER_LISTEN`, `ASMTEACHER_WEB`, `ASMTEACHER_LESSONS`,
`ASMTEACHER_CORS_ORIGINS`.

The server **refuses to start if the curriculum does not validate** — a lesson
whose reference answer is wrong is treated as a fatal configuration error, not a
warning.

## Is it safe to expose publicly?

Yes, behind a proxy that does rate limiting. The reasoning:

**There is no code-execution surface.** The emulator that runs user-submitted
assembly is a pure interpreter with no bridge to the host — the only syscalls it
implements are `write` to file descriptors 1 and 2 (into an in-memory buffer)
and `exit`. It cannot open a file, make a network connection, or affect the
process. There is no `unsafe` anywhere in the workspace, and the executable
parser that consumes uploads is fuzzed to return an error rather than panic on
hostile input. There is no authentication, database, or secret to compromise;
the only server-side secret is the lesson answer keys, and a test asserts they
are never serialised to a client.

So the risk is not compromise — it is denial of service, and that is defended in
two layers.

### What the application guarantees

- **Bounded CPU per request.** Every interpreter run is capped at 500,000 steps
  (a teaching program uses a few thousand); the assembler's branch-relaxation
  loop is bounded; disassembly and parsing are size-limited.
- **Bounded memory per request.** The execution trace is capped at 10,000
  entries *independently of how many steps run*, so asking for more steps cannot
  inflate the response. Inputs are size-limited: 256 KiB of source, 1 MiB of
  machine code, 16 MiB of uploaded executable, 64 memory regions on a step
  request.
- **Nothing blocks the async runtime.** All CPU-bound work — interpret,
  assemble, disassemble, parse, grade — runs on a blocking thread pool, so no
  single request can stall the workers that keep the server answering
  `/health` and shedding load.
- **A 15-second wall-clock timeout** and **panic isolation** on every request.
- **No wildcard CORS.** Same-origin only by default; cross-origin callers must
  be named explicitly with `--cors-origin`.

### What the proxy must provide

Rate limiting and connection limits belong at the edge, where they can be tuned
without redeploying. At minimum:

- **Request rate limiting** on `/api/` — especially `/api/emu/run`,
  `/api/emu/step`, `/api/asm/assemble`, and `/api/binfmt/inspect`, which do real
  work per call.
- **Concurrent-connection caps** per client IP.
- **Client body-size and header/read timeouts** (slow-loris protection).

## nginx example

```nginx
# Rate-limit zones: a steady rate with a small burst.
limit_req_zone  $binary_remote_addr zone=api:10m   rate=10r/s;
limit_conn_zone $binary_remote_addr zone=conns:10m;

server {
    listen 443 ssl http2;
    server_name asmteacher.example;

    ssl_certificate     /etc/letsencrypt/live/asmteacher.example/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/asmteacher.example/privkey.pem;

    # Slow-loris protection.
    client_body_timeout   10s;
    client_header_timeout  10s;
    client_max_body_size  16m;   # matches the inspect upload cap
    limit_conn conns 20;

    location /api/ {
        limit_req  zone=api burst=20 nodelay;
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_read_timeout 20s;   # above the app's 15s request timeout
    }

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
    }
}
```

Caddy equivalent: use the `rate_limit` handler (or the plugin) on `/api/*`, set
`request_body max_size 16MB`, and reverse-proxy the rest.

## CORS

In the default single-origin deployment the frontend is served by the same
process, so no CORS headers are needed and none are sent. Only a **split**
deployment — the frontend on a different host, or the Vite dev server — needs
CORS:

```sh
# Development: the Vite dev server on :5173 calling the API on :8080.
asmteacher --cors-origin http://localhost:5173

# A separately-hosted frontend.
asmteacher --cors-origin https://asmteacher.example
```

The value is an explicit origin allowlist. A wildcard is never emitted. Do not
add a cross-origin allowance you do not need, and if you ever introduce
cookie-based authentication, revisit this — a permissive origin plus credentials
is a CSRF hazard.

## Reproducible build

```sh
docker build -t asmteacher-dev -f contrib/Dockerfile .
docker run --rm -v "$PWD":/work asmteacher-dev contrib/build.sh
```

The image carries the full toolchain (Rust, Node, nasm, binutils, gcc, gdb), so
the differential tests that validate the disassembler against `objdump` and the
assembler against `nasm` actually run rather than skipping.
