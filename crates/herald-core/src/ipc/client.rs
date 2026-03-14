use std::path::Path;
use tokio::net::UnixStream;

use crate::error::Result;
use crate::ipc::protocol::{read_message, write_message, IpcRequest, IpcResponse};

pub struct IpcClient;

impl IpcClient {
    pub async fn send(socket_path: &Path, request: &IpcRequest) -> Result<IpcResponse> {
        let mut stream = UnixStream::connect(socket_path).await?;
        write_message(&mut stream, request).await?;
        let response: IpcResponse = read_message(&mut stream).await?;
        Ok(response)
    }
}
