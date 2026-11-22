"""Install CA bundles and an EST client identity through the gateway's HTTPS API."""

import hashlib
import ssl
import subprocess
from pathlib import Path

import pytest


@pytest.fixture(scope="module")
def identity(tmp_path_factory):
    directory = tmp_path_factory.mktemp("identity")

    def openssl(*args):
        subprocess.run(["openssl", *args], cwd=directory, check=True, capture_output=True)

    for name, issuer in (("root", None), ("issuer", "root"), ("client", "issuer")):
        req = ["req", "-newkey", "rsa:3072", "-nodes", "-subj", f"/CN=Test {name}",
               "-keyout", f"{name}.key"]
        if issuer is None:
            openssl(*req, "-x509", "-days", "2", "-set_serial", "1",
                    "-addext", "basicConstraints=critical,CA:TRUE", "-out", "root.pem")
        else:
            openssl(*req, "-out", f"{name}.csr")
            (directory / "extensions").write_text(
                "basicConstraints=critical,CA:" + ("TRUE" if name == "issuer" else "FALSE") + "\n")
            openssl("x509", "-req", "-in", f"{name}.csr", "-CA", f"{issuer}.pem",
                    "-CAkey", f"{issuer}.key", "-set_serial", "2" if name == "issuer" else "3",
                    "-days", "2", "-extfile", "extensions", "-out", f"{name}.pem")
    return directory


def der(pem):
    return ssl.PEM_cert_to_DER_cert(pem.decode())


def tls_status(api):
    status, body = api("GET", "/tls")
    assert status == 200
    return body


def test_trust_bundle_import(api, identity):
    roots = Path(__file__).resolve().parents[2] / "gateway-hardware" / "certs"
    certs = [der((identity / f"{name}.pem").read_bytes()) for name in ("issuer", "root")]
    for root in ("x1", "x2", "ye", "yr"):
        certs.append((roots / f"isrg-root-{root}.der").read_bytes())
    bundle = b"".join(certs)
    pem = "".join(ssl.DER_cert_to_PEM_cert(cert) for cert in certs).encode()
    assert 5 * 1024 < len(bundle) <= 6 * 1024
    path = "/tls/trust-anchor"
    for payload in (pem, bundle):
        assert api("PUT", path, payload)[0] == 204
        status, system = api("GET", "/system")
        assert status == 200
        print(f"Trust upload: {len(payload)} B, {len(bundle)} B DER, peak heap {system['heap_peak_used']} B")
        installed = tls_status(api)["trust_anchor"]
        assert installed["count"] == len(certs)
        assert installed["fingerprint_sha256"] == hashlib.sha256(certs[0]).hexdigest()
    # A full workspace accepts a PEM with trailing whitespace. An oversized
    # upload must release the workspace before serializing its JSON error.
    full = pem.ljust(10 * 1024, b"\n")
    assert api("PUT", path, full)[0] == 204
    status, error = api("PUT", path, full + b"\n")
    assert status == 413 and error["error"] == "bad_size"
    assert api("PUT", path, certs[0] + bytes.fromhex("3000"))[0] == 422
    assert api("PUT", path, bundle + certs[0])[0] == 422
    assert tls_status(api)["trust_anchor"] == installed
    assert api("DELETE", path)[0] == 204
    assert tls_status(api)["trust_anchor"] is None


def test_est_identity_import(api, identity):
    chain = [(identity / f"{name}.pem").read_bytes() for name in ("client", "issuer", "root")]
    key = (identity / "client.key").read_bytes()
    path = "/tls/est/identity"
    assert api("PUT", path, chain[0] + key + b"".join(chain[1:]))[0] == 204
    status, system = api("GET", "/system")
    assert status == 200
    chain_size = sum(len(der(cert)) for cert in chain)
    print(f"Identity: {chain_size} B DER, peak heap {system['heap_peak_used']} B")
    installed = tls_status(api)["est"]["identity"]
    assert installed["fingerprint_sha256"] == hashlib.sha256(der(chain[0])).hexdigest()
    assert api("PUT", path, key)[0] == 400
    assert tls_status(api)["est"]["identity"] == installed
    assert api("DELETE", path)[0] == 204
    assert tls_status(api)["est"]["identity"] is None
