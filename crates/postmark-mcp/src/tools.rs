//! Tool catalogue. Each module holds one area of the Postmark API as its own
//! `#[tool_router]` block; `PostmarkMcpServer::new` merges them.

mod server;
