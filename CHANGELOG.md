# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once a first public release is tagged.

## [Unreleased]

### Added

- Cargo workspace with `postmark-mcp-core`, `postmark-mcp-client`, and `postmark-mcp`.
- `HardenedTransport`, an HTTP client for the Postmark API that confines requests to one origin, refuses redirects, and bounds request time and response size.
- Environment configuration compatible with the official `postmark-mcp` server.
- MCP server over stdio, with the `get_server_info` tool, which removes credentials embedded in hook URLs.
- `postmark-mcp doctor`, which validates configuration and checks the token against Postmark.
