"""Rotate HTTPS credentials while an authenticated connection still uses the old ones."""


def test_replace_active_certificate(api):
    status, before = api("GET", "/system")
    assert status == 200
    status, original = api("GET", "/tls")
    assert status == 200
    assert api("POST", "/tls/self-signed", {"validity_years": 2})[0] == 204
    status, replaced = api("GET", "/tls")
    assert status == 200
    assert replaced["certificate"]["fingerprint_sha256"] != original["certificate"]["fingerprint_sha256"]
    assert replaced["key_pin_sha256"] == original["key_pin_sha256"]
    status, after = api("GET", "/system")
    assert status == 200 and after["boot_id"] == before["boot_id"]
    pool = after["crypto_pool"]
    assert 0 < pool["used"] <= pool["peak"] < pool["size"]
    assert pool["failures"] == 0
