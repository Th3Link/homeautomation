# Built-in root certificates

The certificate authorities the firmware trusts out of the box
(`src/cert/roots.rs`, ADR 0020): Let's Encrypt's (ISRG's) roots, DER, from
<https://letsencrypt.org/certificates/>.

| File | CA | Key | SHA-256 of the DER |
|---|---|---|---|
| `isrg-root-x1.der` | ISRG Root X1 | RSA 4096 | `96bcec06264976f37460779acf28c5a7cfe8a3c0aae11a8ffcee05c0bddf08c6` |
| `isrg-root-x2.der` | ISRG Root X2 | ECDSA P-384 | `69729b8e15a86efc177a57afb7171dfc64add28c2fca8cf1507e34453ccb1470` |
| `isrg-root-ye.der` | Root YE | ECDSA P-384 | `e14ffcad5b0025731006caa43a121a22d8e9700f4fb9cf852f02a708aa5d5666` |
| `isrg-root-yr.der` | Root YR | RSA 4096 | `e57b7e6f150c419102e8d5c055729ff967b9d1a829bf00cec89ca604ebf4a86f` |

X1 and X2 are in every current trust store; the files match the ones in
Mozilla's (`ca-certificates`). Root YE and Root YR head the hierarchy Let's
Encrypt issues from (intermediates YE1/YE2, YR1/YR2); they aren't in the
trust stores yet, and are cross-signed by X2 and X1. The files were checked
against those cross-signed certificates (same subject and key, signature
by X2 and X1 from the system trust store), so trusting them adds nothing
X1 and X2 don't already allow; they only let chains that end at them
verify.

To check a file:

```sh
openssl x509 -inform DER -in isrg-root-x1.der -noout -subject -fingerprint -sha256
```
