#!/usr/bin/env bash
# Run from any directory with chronyd and OpenSSL on PATH.
set -euo pipefail
cd "$(dirname "$0")/.."
work=$(mktemp -d)
cleanup() {
    if [[ -n ${server_pid:-} ]]; then
        kill "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
    fi
    rm -rf "$work"
}
trap cleanup EXIT

# Private test ports, independent of any system clock service.
export NTS_TEST_CA="$work/ca.der"
export NTS_TEST_KE_PORT=${NTS_TEST_KE_PORT:-24460}
export NTS_TEST_UDP_PORT=${NTS_TEST_UDP_PORT:-24123}
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 1 \
    -subj '/CN=NTS test CA' -addext 'basicConstraints=critical,CA:TRUE' \
    -keyout "$work/ca.key" -out "$work/ca.pem" 2>/dev/null
openssl x509 -in "$work/ca.pem" -outform DER -out "$NTS_TEST_CA"
openssl req -new -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -subj '/CN=localhost' \
    -keyout "$work/server.key" -out "$work/server.csr" 2>/dev/null
printf '%s\n' 'subjectAltName=IP:127.0.0.1' 'extendedKeyUsage=serverAuth' > "$work/extensions"
openssl x509 -req -in "$work/server.csr" -CA "$work/ca.pem" -CAkey "$work/ca.key" \
    -CAcreateserial -days 1 -extfile "$work/extensions" -out "$work/server.pem" 2>/dev/null
cat > "$work/chrony.conf" <<CONF
bindaddress 127.0.0.1
allow 127.0.0.1
local stratum 8
port $NTS_TEST_UDP_PORT
ntsport $NTS_TEST_KE_PORT
ntsservercert $work/server.pem
ntsserverkey $work/server.key
cmdport 0
bindcmdaddress $work/chrony.sock
pidfile $work/chrony.pid
CONF
chronyd -4 -d -x -U -u "$(id -un)" -f "$work/chrony.conf" > "$work/server.log" 2>&1 &
server_pid=$!
for _ in {1..100}; do
    if ! kill -0 "$server_pid" 2>/dev/null; then cat "$work/server.log"; exit 1; fi
    if (echo > "/dev/tcp/127.0.0.1/$NTS_TEST_KE_PORT") 2>/dev/null; then break; fi
    sleep 0.1
done
cargo test --locked --all-features --test chrony -- --ignored || { cat "$work/server.log"; exit 1; }
