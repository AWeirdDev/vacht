// BOO! JOB APPLICATION!!!! HHAHAHAHAH yeah wtf

use std::sync::Arc;

use interprocess::local_socket::traits::tokio::Listener;
use v8::{Global, Script};

use crate::{
    socket::{self, LocalSocketStream, PythonEvent},
    state::IsolateState,
};

macro_rules! create_trycatch {
    ($name:ident for $context_scope:expr) => {
        let $name = std::pin::pin!(v8::TryCatch::new($context_scope));
        let $name = &mut $name.init();
    };
}

macro_rules! ensure {
    ($tt:tt, try in ($stream:expr, $try_catch:expr) => $do:expr) => {{
        if let Some(data) = $do {
            data
        } else {
            $stream
                .send_js_exception($try_catch, $try_catch.exception().unwrap())
                .await?;
            continue $tt;
        }
    }};
}

pub async fn start_job(stream: &mut LocalSocketStream) -> Result<(), Box<dyn core::error::Error>> {
    let isolate = &mut v8::Isolate::new(v8::CreateParams::default());

    let handle_scope = std::pin::pin!(v8::HandleScope::new(isolate));
    let handle_scope = &mut handle_scope.init();
    let context = v8::Context::new(handle_scope, Default::default());

    let mut context_scope_ref = Box::new(v8::ContextScope::new(handle_scope, context));
    let context_scope = context_scope_ref.as_mut();

    // safety: `state` drops before the scope
    let state = Arc::new(IsolateState::new(context_scope));

    'block: while let Some(event) = stream.receive().await? {
        tracing::info!(?event, "got event");

        match event {
            PythonEvent::Errored(e) => {
                stream.send_error(&e.to_string()).await?;
            }

            PythonEvent::RunScript(script) => {
                // attempt to compile and run
                create_trycatch!(try_catch for context_scope);

                tracing::info!("running");
                let source = ensure!('block, try in (stream, try_catch) => v8::String::new(try_catch, &script));
                tracing::info!("finished source");
                let script = ensure!('block, try in (stream, try_catch) => Script::compile(try_catch, source.cast(), None));
                tracing::info!("finished script");
                let result = ensure!('block, try in (stream, try_catch) => script.run(try_catch));
                let index = {
                    let inner_state = state.clone();
                    let mut arena = inner_state.arena.lock().await;
                    arena.alloc(Global::new(try_catch, result))
                };
                stream.send_js_value_id(index).await?;
            }

            PythonEvent::DropValue(ident) => {
                tracing::info!("dropping value from arena: {}", ident);
                let inner_state = state.clone();
                let mut arena = inner_state.arena.lock().await;
                // doesn't matter if it doesn't exist
                if arena.dealloc(&inner_state.ctx_scope, ident).await {
                    tracing::info!("value dropped");
                }
            }
        }
    }

    // notify the python side that we're closing
    stream.send_closing().await.ok();

    // do cleanup
    tracing::info!("cleaning up isolate state...");
    state.close().await;

    Ok(())
}

pub async fn start_server() -> anyhow::Result<()> {
    {
        tracing::info!("initializing v8");
        let platform = v8::new_default_platform(0, false).make_shared();
        v8::V8::initialize_platform(platform);
        v8::V8::initialize();
        tracing::info!("v8 initialized");
    }

    let (printname, name) = socket::get_name()?;
    let listener = socket::create_listener(name)?;

    tracing::info!("server running at {}", printname);
    loop {
        let conn = match listener.accept().await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("an error occurred while accepting: {e}");
                continue;
            }
        };

        std::thread::spawn(move || {
            // sadly, we can only run this in the current thread!
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            tracing::info!("created a new isolate instance");
            let local = tokio::task::LocalSet::new();
            let mut stream = LocalSocketStream::new(conn);
            if let Err(why) = rt.block_on(local.run_until(start_job(&mut stream))) {
                // notify python side of the error occurrence
                rt.block_on(local.run_until(stream.send_error(&why.to_string())))
                    .ok();
                tracing::error!(?why, "error while spawning handler");
            }
            tracing::info!("isolate instance dropped");
        });
    }
}
