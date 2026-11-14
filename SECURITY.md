# Security policy

ezmark renders untrusted files, so security reports are welcome and taken
seriously.

## Reporting a vulnerability

Please report privately through GitHub's
[private vulnerability reporting](https://github.com/Vibin-LLC-2025/ezmark/security/advisories/new)
rather than in a public issue. Include the markdown file or steps that
reproduce the problem, the OS and version, and what you observed.

You can expect an acknowledgement within a week. There is no bug bounty.

## Supported versions

ezmark has no release yet. Fixes land on `main`.

## What counts

The contract is in [ADR 0003](docs/adr/0003-security-contract.md). Anything
that breaks it is in scope, for example:

- script execution from a document, however it gets there
- a document making the app load a remote resource or open a network connection
- reading a file outside the document root, or reaching a UNC or device path
- the webview navigating away from the app page
- an IPC command callable from a window or context it was not granted to
- any `test-harness` code reaching a release build

Out of scope: behaviour that needs a modified binary, and issues in WebView2,
WKWebView, or the OS itself (please report those upstream; a mitigation in
ezmark is still welcome).
