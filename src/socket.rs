use interprocess::local_socket::{
    self, GenericFilePath, ListenerOptions, Name,
    tokio::{Listener, RecvHalf, SendHalf, Stream, prelude::*},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub type IoResult<T> = Result<T, std::io::Error>;

#[repr(u8)]
#[derive(Debug)]
pub enum RustEventType {
    /// An error occurred.
    ///
    /// `[str REASON]`
    Error = 0,

    /// The isolate is closing.
    Closing = 1,

    /// Javascript exception.
    /// `[str NAME][str MESSAGE][str STACK]`
    JsException = 2,

    /// Value handle.
    JsValue = 3,
}

impl RustEventType {
    pub const fn from_u8(x: u8) -> Option<Self> {
        use RustEventType::*;
        match x {
            0 => Some(Error),
            1 => Some(Closing),
            2 => Some(JsException),
            _ => None,
        }
    }
}

#[repr(u8)]
#[derive(Debug)]
pub enum PythonEventType {
    CloseIsolate = 0,
    RunScript = 1,
    DropValue = 2,
    Orchestrate = 3,
}

impl PythonEventType {
    pub const fn from_u8(x: u8) -> Option<Self> {
        use PythonEventType::*;
        match x {
            0 => Some(CloseIsolate),
            1 => Some(RunScript),
            2 => Some(DropValue),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum PythonEvent {
    Errored(LocalSocketStreamError),
    RunScript(String),
    DropValue(usize),
}

#[derive(Debug, thiserror::Error)]
pub enum LocalSocketStreamError {
    #[error("unknown event type")]
    UnknownEventType,
}

pub struct LocalSocketStream {
    rx: RecvHalf,
    tx: SendHalf,
}

impl LocalSocketStream {
    #[inline(always)]
    pub fn new(stream: Stream) -> Self {
        let (rx, tx) = stream.split();
        Self { rx, tx }
    }

    #[inline(always)]
    pub async fn send_string(&mut self, s: &str) -> IoResult<()> {
        // this is cheap af
        let bytes = s.as_bytes();

        // we need to tell it how long the string is
        self.tx.write_u32_le(bytes.len() as u32).await?;

        // directly write the bytes here & there
        self.tx.write_all(bytes).await?;

        Ok(())
    }

    #[inline(always)]
    pub async fn read_string(&mut self) -> IoResult<String> {
        let length = self.rx.read_u32_le().await?;
        let mut buf = vec![0u8; length as usize];
        self.rx.read_exact(&mut buf).await?;
        Ok(String::from_utf8(buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?)
    }

    #[inline(always)]
    pub async fn send_error(&mut self, reason: &str) -> IoResult<()> {
        // first, we say 'fuck you, there's an error'
        self.tx.write_u8(RustEventType::Error as u8).await?;
        self.send_string(reason).await?;
        self.tx.flush().await?;
        Ok(())
    }

    #[inline(always)]
    pub async fn send_js_value_id(&mut self, id: usize) -> IoResult<()> {
        self.tx.write_u8(RustEventType::JsValue as u8).await?;
        self.tx.write_u64_le(id as u64).await?;
        self.tx.flush().await?;
        Ok(())
    }

    #[inline(always)]
    pub async fn send_js_exception<'s>(
        &mut self,
        scope: &v8::PinScope<'s, '_>,
        exc: v8::Local<'s, v8::Value>,
    ) -> IoResult<()> {
        self.tx.write_u8(RustEventType::JsException as u8).await?;

        let exception = exc.cast::<v8::Object>();
        let name = exception
            .get(
                scope,
                v8::String::new(scope, "name").unwrap().cast::<v8::Value>(),
            )
            .map(|s| s.to_rust_string_lossy(scope))
            .unwrap_or_else(|| "Error".to_string());
        let message = exception
            .get(
                scope,
                v8::String::new(scope, "message")
                    .unwrap()
                    .cast::<v8::Value>(),
            )
            .map(|s| s.to_rust_string_lossy(scope))
            .unwrap_or_else(|| "unknown message".to_string());
        let stack = {
            let value = exception
                .get(
                    scope,
                    v8::String::new(scope, "stack").unwrap().cast::<v8::Value>(),
                )
                .unwrap_or_else(|| v8::undefined(scope).cast::<v8::Value>());

            if value.is_null_or_undefined() {
                None
            } else {
                Some(value.to_rust_string_lossy(scope))
            }
        };

        self.send_string(&name).await?;
        self.send_string(&message).await?;
        self.send_string(&stack.unwrap_or_else(|| "".to_string()))
            .await?;
        self.tx.flush().await?;

        Ok(())
    }

    #[inline(always)]
    pub async fn send_closing(&mut self) -> IoResult<()> {
        self.tx.write_u8(RustEventType::Closing as u8).await
    }

    pub async fn receive(&mut self) -> anyhow::Result<Option<PythonEvent>> {
        // first we get the request type
        let typ = self.rx.read_u8().await?;
        let Some(ev) = PythonEventType::from_u8(typ) else {
            // unknown event type, aborting
            return Ok(Some(PythonEvent::Errored(
                LocalSocketStreamError::UnknownEventType,
            )));
        };
        tracing::info!("found event type: {:?}", ev);

        Ok(match ev {
            PythonEventType::CloseIsolate => None,
            PythonEventType::RunScript => Some(PythonEvent::RunScript(self.read_string().await?)),
            PythonEventType::DropValue => Some(PythonEvent::DropValue(
                self.rx.read_u64_le().await? as usize,
            )),
            PythonEventType::Orchestrate => todo!(),
        })
    }
}

#[inline(always)]
pub fn get_name<'s>() -> anyhow::Result<(&'static str, local_socket::Name<'s>)> {
    // if GenericNamespaced::is_supported() {
    //     Ok((
    //         "vacht.sock",
    //         "vacht.sock".to_ns_name::<GenericNamespaced>()?,
    //     ))
    // } else {
    Ok((
        "/tmp/vacht.sock",
        "/tmp/vacht.sock".to_fs_name::<GenericFilePath>()?,
    ))
    // }
}

#[inline(always)]
pub fn create_listener(name: Name<'_>) -> anyhow::Result<Listener> {
    Ok(ListenerOptions::new()
        .name(name)
        .try_overwrite(true)
        .create_tokio()?)
}
