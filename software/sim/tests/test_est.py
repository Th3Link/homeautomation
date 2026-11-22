"""Authenticated EST enrollment against a local CA, including certificate renewal."""

import base64
import errno
import os
import socket
import ssl
import subprocess
import threading
import time
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import pytest


@contextmanager
def time_server():
    """QEMU reaches this peer at 10.0.2.2. EST needs a known clock."""
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as peer:
        try:
            peer.bind(("127.0.0.1", int(os.environ.get("SIM_SNTP_PORT", "123"))))
        except OSError as error:
            if error.errno in (errno.EACCES, errno.EADDRINUSE):
                pytest.skip("EST needs an available local SNTP port (UDP 123)")
            raise
        stop = threading.Event()
        peer.settimeout(0.2)

        def serve():
            while not stop.is_set():
                try:
                    data, addr = peer.recvfrom(128)
                except socket.timeout:
                    continue
                if len(data) >= 48:
                    reply = bytearray(48)
                    reply[:2] = bytes((0x24, 2))
                    reply[24:32] = data[40:48]
                    reply[40:44] = (int(time.time()) + 2_208_988_800).to_bytes(4, "big")
                    peer.sendto(reply, addr)

        worker = threading.Thread(target=serve)
        worker.start()
        try:
            yield
        finally:
            stop.set()
            worker.join()


@pytest.fixture(scope="session")
def sim(start_sim, tmp_path_factory):
    # Keep the new identity and trust store confined to this guest. Seed its
    # time server before boot so enrollment does not wait for public DNS.
    topology = tmp_path_factory.mktemp("est") / "est.toml"
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    topology.write_text(f'[[gateway]]\nname = "gw"\nbus = "vcan-test"\nhttps = {port}\n'
                        'console = ["ntp-server 10.0.2.2"]\n')
    with time_server(), start_sim(topology) as installation:
        yield installation


def test_est_with_installed_and_builtin_roots(api, tmp_path):
    def openssl(*args, data=None):
        return subprocess.run(["openssl", *args], cwd=tmp_path, input=data,
                              capture_output=True, check=True).stdout

    def pem(name):
        return (tmp_path / name).read_bytes()

    def issue(name, issuer, extension):
        openssl("req", "-new", *key_args, "-subj", f"/CN={name}",
                "-keyout", f"{name}.key", "-out", f"{name}.csr")
        (tmp_path / "extensions").write_text(extension + "\n")
        openssl("x509", "-req", "-in", f"{name}.csr", "-CA", issuer + ".crt",
                "-CAkey", issuer + ".key", "-extfile", "extensions",
                "-days", "2", "-out", name + ".crt")

    key_args = ("-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:P-256", "-nodes")
    openssl("req", "-x509", *key_args, "-days", "2", "-subj", "/CN=QEMU CA",
            "-keyout", "ca.key", "-out", "ca.crt")
    issue("server", "ca", "subjectAltName=IP:10.0.2.2,DNS:10.0.2.2")
    issue("issuer", "ca", "basicConstraints=critical,CA:TRUE")
    issue("client", "issuer", "basicConstraints=critical,CA:FALSE")
    peers = []

    class Responder(BaseHTTPRequestHandler):
        def do_POST(self):
            peers.append((self.path, self.connection.getpeercert(binary_form=True)))
            csr = base64.b64decode(self.rfile.read(int(self.headers["Content-Length"])))
            openssl("x509", "-req", "-inform", "DER", "-CA", "ca.crt", "-CAkey", "ca.key",
                    "-copy_extensions", "copy", "-days", "2", "-out", "issued.crt", data=csr)
            body = base64.b64encode(openssl("crl2pkcs7", "-nocrl", "-certfile", "issued.crt",
                                           "-certfile", "ca.crt", "-outform", "DER"))
            self.send_response(200)
            self.send_header("Transfer-Encoding", "chunked")
            self.end_headers()
            for chunk in (body[:37], body[37:]):
                self.wfile.write(f"{len(chunk):x}\r\n".encode() + chunk + b"\r\n")
            self.wfile.write(b"0\r\n\r\n")

    def wait_for(check):
        deadline = time.monotonic() + 40
        while time.monotonic() < deadline:
            status, state = api("GET", "/tls")
            assert status == 200
            if check(state):
                return
            time.sleep(0.5)
        pytest.fail(f"EST timeout: {state['est']}")

    with ThreadingHTTPServer(("127.0.0.1", 0), Responder) as server:
        tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        tls.load_cert_chain(tmp_path / "server.crt", tmp_path / "server.key")
        tls.load_verify_locations(cafile=tmp_path / "ca.crt")
        tls.verify_mode = ssl.CERT_REQUIRED
        server.socket = tls.wrap_socket(server.socket, server_side=True)
        worker = threading.Thread(target=server.serve_forever)
        worker.start()
        try:
            boot = api("GET", "/system")[1]["boot_id"]
            wait_for(lambda state: state["time_known"])
            directory = Path(__file__).resolve().parents[2] / "gateway-hardware/certs"
            roots = [ssl.DER_cert_to_PEM_cert((directory / f"isrg-root-{r}.der").read_bytes()).encode()
                     for r in ("x1", "x2", "ye", "yr")]
            assert api("PUT", "/tls/trust-anchor", b"".join([*roots, pem("ca.crt")]))[0] == 204
            identity = pem("client.crt") + pem("client.key") + pem("issuer.crt")
            assert api("PUT", "/tls/est/identity", identity)[0] == 204
            url = f"https://10.0.2.2:{server.server_port}/.well-known/est"
            assert api("PATCH", "/tls", {"mode": "est", "fqdn": "qemu-gateway.local",
                                         "est_url": url, "built_in_roots": True})[0] == 204
            wait_for(lambda state: state["certificate"]["origin"] == "est")
            assert api("POST", "/tls/est/enroll")[0] == 202
            wait_for(lambda state: len(peers) == 2 and state["est"]["last_result"] == "ok")
            assert [path.rsplit("/", 1)[1] for path, _ in peers] == ["simpleenroll", "simplereenroll"]
            assert peers[0][1] == ssl.PEM_cert_to_DER_cert(pem("client.crt").decode())
            assert peers[1][1] != peers[0][1]
            system = api("GET", "/system")[1]
            assert system["boot_id"] == boot and system["crypto_pool"]["failures"] == 0
            print(f"EST heap peak {system['heap_peak_used']} B, TLS pool peak {system['crypto_pool']['peak']} B")
        finally:
            server.shutdown()
            worker.join()
