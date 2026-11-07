# Code Review Lens

Review the diff, not the author's intent. Report concrete findings ordered by severity, with file/line evidence and the violated invariant.

## Correctness and integrity

- Does the tool do exactly what its name, description, and annotations promise?
- Can a retry, timeout, or cancellation send an email twice or leave a half-applied change?
- Are partial batch failures reported per item rather than collapsed?
- Are error codes stable and actionable rather than collapsed into strings?

## Security and privacy

- Does every request go through `HardenedTransport`, and stay on the configured origin?
- Can a tool result, log, or error disclose a token, an email body, or a full address?
- Do values from tool calls reach request paths and headers only after validation?
- Are limits enforced before allocation, and are annotations set by the MCP specification's definitions?

## Architecture and Rust

- Is dependency direction preserved and are handlers thin?
- Does every project type conversion use `From` or `TryFrom`, without ad hoc conversion helpers or unchecked semantic casts?
- Are traits narrow, object-safe when needed, and owned by the consumer/domain?
- Is blocking work kept off async executor threads?
- Is production code free of unsafe, panic shortcuts, debug output, and broad
  lint suppressions?

## Tests

- Is there evidence of the right test at the right layer, including the failure path and prohibited side effects?
- Is HTTP behaviour tested against a real local server rather than a fake transport?
- Are time, network, environment, and ordering deterministic?
- Would the test fail if the new production behaviour were removed?

If there are no findings, state that explicitly and list residual untested risks. Do not inflate style preferences into defects.

Before concluding, consider the change holistically. Do not accept a narrow local improvement that transfers risk or complexity into security, another crate, another platform, operations, users, tests, or future maintenance.
