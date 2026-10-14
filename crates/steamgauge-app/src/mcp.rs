//! A program that speaks the Model Context Protocol steering `SteamGauge`.
//!
//! The open app owns the library, the work board and the window, so it is the server: it listens
//! on a local socket only this user can reach, and every tool there is something the window can
//! do. `steamgauge mcp` is what a client starts. It answers the client itself, offering the app's
//! tools whether the app is open or not, and hands each call to the open app. It never opens the
//! app: a closed app stays closed, and a call made while it is says so, so the assistant asks the
//! person to open it.

use std::{
    io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use interprocess::local_socket::{Name, tokio::Stream, traits::tokio::Stream as _};
use rmcp::{
    ErrorData, RoleClient, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
        PaginatedRequestParams, ServerConfig,
    },
    service::{RequestContext, RunningService, ServiceError},
};
use tokio::sync::Mutex;

use crate::update::IDENTIFIER;

/// What a call answers while the app is closed.
pub const CLOSED: &str = "SteamGauge is not open, and it works only while its window is. Ask the \
person to open SteamGauge, then call this again.";

/// What a call answers when the app closed before it replied.
pub const CLOSED_DURING: &str = "SteamGauge closed before it answered, so whether this was done \
is unknown. Ask the person to open SteamGauge, and look before calling this again.";

/// The app's local data folder, the one Tauri names from the same identifier.
pub fn local_data() -> Result<PathBuf> {
    Ok(dirs::data_local_dir()
        .context("this system names no folder for the app's local data")?
        .join(IDENTIFIER))
}

/// Where the open app writes the name of its socket. The folder is the user's own, so the name
/// is as private as the folder: a program another user runs can neither read it to reach the
/// app nor take it first to stand in for the app.
pub fn rendezvous(local: &Path) -> PathBuf {
    local.join("mcp-socket")
}

/// The name a socket is reached by, from what the rendezvous holds: a pipe's name on Windows, a
/// file's path elsewhere.
pub fn socket_name(written: &str) -> io::Result<Name<'static>> {
    #[cfg(windows)]
    {
        use interprocess::local_socket::{GenericNamespaced, ToNsName};
        written.to_owned().to_ns_name::<GenericNamespaced>()
    }
    #[cfg(not(windows))]
    {
        use interprocess::local_socket::{GenericFilePath, ToFsName};
        PathBuf::from(written).to_fs_name::<GenericFilePath>()
    }
}

async fn connect(local: &Path) -> io::Result<Stream> {
    let written = std::fs::read_to_string(rendezvous(local))?;
    Stream::connect(socket_name(written.trim())?).await
}

/// The client's server, standing in front of whichever app is open.
pub(crate) struct Bridge {
    local: PathBuf,
    app: Mutex<Option<RunningService<RoleClient, ()>>>,
}

impl Bridge {
    pub(crate) fn new(local: PathBuf) -> Self {
        Self {
            local,
            app: Mutex::new(None),
        }
    }

    /// A connection to the open app: the one held while it lasts, or a new one, since the app
    /// may have closed and opened again since the last call.
    async fn app(&self) -> Option<rmcp::service::Peer<RoleClient>> {
        let mut held = self.app.lock().await;
        if held
            .as_ref()
            .is_some_and(|running| running.is_closed() || running.peer().is_transport_closed())
        {
            *held = None;
        }
        if held.is_none() {
            let stream = connect(&self.local).await.ok()?;
            *held = Some(().serve(stream).await.ok()?);
        }
        held.as_ref().map(|running| running.peer().clone())
    }

    pub(crate) async fn call(
        &self,
        request: CallToolRequestParams,
    ) -> Result<CallToolResponse, ErrorData> {
        let Some(app) = self.app().await else {
            return Ok(closed(CLOSED));
        };
        // Never sent twice: a call the app may have started is reported, not repeated.
        match app.call_tool_once(request).await {
            Ok(answer) => Ok(answer),
            Err(ServiceError::McpError(error)) => Err(error),
            Err(_) => {
                *self.app.lock().await = None;
                Ok(closed(CLOSED_DURING))
            }
        }
    }
}

fn closed(why: &str) -> CallToolResponse {
    CallToolResult::error(vec![ContentBlock::text(why)]).into()
}

impl ServerHandler for Bridge {
    fn get_info(&self) -> ServerConfig {
        crate::ui::mcp_server_config()
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        std::future::ready(Ok(crate::ui::mcp_tool_list()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        self.call(request).await
    }
}

/// `steamgauge mcp`: the client's server on standard input and output, until the client closes.
pub async fn bridge() -> Result<()> {
    let running = Bridge::new(local_data()?)
        .serve(rmcp::transport::stdio())
        .await
        .context("could not answer the client")?;
    running.waiting().await.context("the server stopped")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rendezvous_is_in_the_apps_own_folder() {
        let local = Path::new("local").join(IDENTIFIER);
        assert_eq!(rendezvous(&local), local.join("mcp-socket"));
    }

    #[test]
    fn a_written_name_reaches_a_socket() {
        #[cfg(windows)]
        assert!(socket_name("steamgauge-mcp-0123456789abcdef").is_ok());
        #[cfg(not(windows))]
        assert!(socket_name("/tmp/steamgauge-mcp/socket").is_ok());
    }
}
