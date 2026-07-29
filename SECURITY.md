# Security Policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub Security Advisories for
this repository. Do not include exploit details in a public issue.

## HTTP service boundary

`apple-matting-cli --server` is intended for local or trusted-network use. It
listens on `0.0.0.0`, enables permissive CORS, and does not provide
authentication, TLS, upload-size limits, rate limits, queueing, or a global
concurrency cap. Add those controls before exposing it to untrusted clients.
