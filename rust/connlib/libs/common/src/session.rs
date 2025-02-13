use async_trait::async_trait;
use backoff::{backoff::Backoff, ExponentialBackoffBuilder};
use boringtun::x25519::{PublicKey, StaticSecret};
use parking_lot::Mutex;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use rand_core::OsRng;
use std::{
    marker::PhantomData,
    net::{Ipv4Addr, Ipv6Addr},
    sync::Arc,
    time::Duration,
};
use tokio::{runtime::Runtime, sync::mpsc::Receiver};
use url::Url;
use uuid::Uuid;

use crate::{
    control::{MessageResult, PhoenixChannel, PhoenixSenderWithTopic},
    messages::{Key, ResourceDescription, ResourceDescriptionCidr},
    Error, Result,
};

pub const DNS_SENTINEL: Ipv4Addr = Ipv4Addr::new(100, 100, 111, 1);

// TODO: Not the most tidy trait for a control-plane.
/// Trait that represents a control-plane.
#[async_trait]
pub trait ControlSession<T, CB: Callbacks> {
    /// Start control-plane with the given private-key in the background.
    async fn start(
        private_key: StaticSecret,
        receiver: Receiver<MessageResult<T>>,
        control_signal: PhoenixSenderWithTopic,
        callbacks: CB,
    ) -> Result<()>;

    /// Either "gateway" or "client" used to get the control-plane URL.
    fn socket_path() -> &'static str;
}

// TODO: Currently I'm using Session for both gateway and clients
// however, gateway could use the runtime directly and could make things easier
// so revisit this.
/// A session is the entry-point for connlib, maintains the runtime and the tunnel.
///
/// A session is created using [Session::connect], then to stop a session we use [Session::disconnect].
pub struct Session<T, U, V, R, M, CB: Callbacks> {
    runtime: Arc<Mutex<Option<Runtime>>>,
    callbacks: CB,
    _phantom: PhantomData<(T, U, V, R, M)>,
}

/// Resource list that will be displayed to the users.
#[derive(Debug)]
pub struct ResourceList {
    pub resources: Vec<String>,
}

/// Tunnel addresses to be surfaced to the client apps.
#[derive(Debug)]
pub struct TunnelAddresses {
    /// IPv4 Address.
    pub address4: Ipv4Addr,
    /// IPv6 Address.
    pub address6: Ipv6Addr,
}

/// Traits that will be used by connlib to callback the client upper layers.
pub trait Callbacks: Clone + Send + Sync {
    /// Called when the tunnel address is set.
    fn on_set_interface_config(&self, tunnel_addresses: TunnelAddresses, dns_address: Ipv4Addr);
    /// Called when the tunnel is connected.
    fn on_tunnel_ready(&self);
    /// Called when when a route is added.
    fn on_add_route(&self, route: String);
    /// Called when when a route is removed.
    fn on_remove_route(&self, route: String);
    /// Called when the resource list changes.
    fn on_update_resources(&self, resource_list: ResourceList);
    /// Called when the tunnel is disconnected.
    ///
    /// If the tunnel disconnected due to a fatal error, `error` is the error
    /// that caused the disconnect.
    fn on_disconnect(&self, error: Option<&Error>);
    /// Called when there's a recoverable error.
    fn on_error(&self, error: &Error);
}

macro_rules! fatal_error {
    ($result:expr, $rt:expr, $cb:expr) => {
        match $result {
            Ok(res) => res,
            Err(err) => {
                Self::disconnect_inner($rt, $cb, Some(err));
                return;
            }
        }
    };
}

impl<T, U, V, R, M, CB> Session<T, U, V, R, M, CB>
where
    T: ControlSession<M, CB>,
    U: for<'de> serde::Deserialize<'de> + std::fmt::Debug + Send + 'static,
    R: for<'de> serde::Deserialize<'de> + std::fmt::Debug + Send + 'static,
    V: serde::Serialize + Send + 'static,
    M: From<U> + From<R> + Send + 'static + std::fmt::Debug,
    CB: Callbacks + 'static,
{
    /// Block on waiting for ctrl+c to terminate the runtime.
    /// (Used for the gateways).
    pub fn wait_for_ctrl_c(&mut self) -> Result<()> {
        self.runtime
            .lock()
            .as_ref()
            .ok_or(Error::NoRuntime)?
            .block_on(async {
                tokio::signal::ctrl_c().await?;
                Ok(())
            })
    }

    /// Starts a session in the background.
    ///
    /// This will:
    /// 1. Create and start a tokio runtime
    /// 2. Connect to the control plane to the portal
    /// 3. Start the tunnel in the background and forward control plane messages to it.
    ///
    /// The generic parameter `CB` should implement all the handlers and that's how errors will be surfaced.
    ///
    /// On a fatal error you should call `[Session::disconnect]` and start a new one.
    // TODO: token should be something like SecretString but we need to think about FFI compatibility
    pub fn connect(portal_url: impl TryInto<Url>, token: String, callbacks: CB) -> Result<Self> {
        // TODO: We could use tokio::runtime::current() to get the current runtime
        // which could work with swif-rust that already runs a runtime. But IDK if that will work
        // in all pltaforms, a couple of new threads shouldn't bother none.
        // Big question here however is how do we get the result? We could block here await the result and spawn a new task.
        // but then platforms should know that this function is blocking.

        let this = Self {
            runtime: Mutex::new(Some(
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()?,
            ))
            .into(),
            callbacks,
            _phantom: PhantomData,
        };

        if cfg!(feature = "mock") {
            Self::connect_mock(this.callbacks.clone());
        } else {
            Self::connect_inner(
                Arc::clone(&this.runtime),
                portal_url.try_into().map_err(|_| Error::UriError)?,
                token,
                this.callbacks.clone(),
            );
        }

        Ok(this)
    }

    fn connect_inner(
        runtime: Arc<Mutex<Option<Runtime>>>,
        portal_url: Url,
        token: String,
        callbacks: CB,
    ) {
        let runtime_disconnector = Arc::clone(&runtime);
        runtime.lock().as_ref().unwrap().spawn(async move {
            let private_key = StaticSecret::random_from_rng(OsRng);
            let self_id = uuid::Uuid::new_v4();
            let name_suffix: String = thread_rng().sample_iter(&Alphanumeric).take(8).map(char::from).collect();

            let connect_url = fatal_error!(
                get_websocket_path(portal_url, token, T::socket_path(), &Key(PublicKey::from(&private_key).to_bytes()), &self_id.to_string(), &name_suffix),
                &runtime_disconnector,
                &callbacks
            );


            // This is kinda hacky, the buffer size is 1 so that we make sure that we
            // process one message at a time, blocking if a previous message haven't been processed
            // to force queue ordering.
            let (control_plane_sender, control_plane_receiver) = tokio::sync::mpsc::channel(1);

            let mut connection = PhoenixChannel::<_, U, R, M>::new(connect_url, move |msg| {
                let control_plane_sender = control_plane_sender.clone();
                async move {
                    tracing::trace!("Received message: {msg:?}");
                    if let Err(e) = control_plane_sender.send(msg).await {
                        tracing::warn!("Received a message after handler already closed: {e}. Probably message received during session clean up.");
                    }
                }
            });

            // Used to send internal messages
            let topic = T::socket_path().to_string();
            let internal_sender = connection.sender_with_topic(topic.clone());
            fatal_error!(
                T::start(private_key, control_plane_receiver, internal_sender, callbacks.clone()).await,
                &runtime_disconnector,
                &callbacks
            );

            tokio::spawn(async move {
                let mut exponential_backoff = ExponentialBackoffBuilder::default().build();
                loop {
                    // `connection.start` calls the callback only after connecting
                    let result = connection.start(vec![topic.clone()], || exponential_backoff.reset()).await;
                    if let Some(t) = exponential_backoff.next_backoff() {
                        tracing::warn!("Error during connection to the portal, retrying in {} seconds", t.as_secs());
                        callbacks.on_error(&result.err().unwrap_or(Error::PortalConnectionError(tokio_tungstenite::tungstenite::Error::ConnectionClosed)));
                        tokio::time::sleep(t).await;
                    } else {
                        tracing::error!("Connection to the portal error, check your internet or the status of the portal.\nDisconnecting interface.");
                        fatal_error!(
                            result.and(Err(Error::PortalConnectionError(tokio_tungstenite::tungstenite::Error::ConnectionClosed))),
                            &runtime_disconnector,
                            &callbacks
                        );
                    }
                }

            });

        });
    }

    fn connect_mock(callbacks: CB) {
        std::thread::sleep(Duration::from_secs(1));
        callbacks.on_set_interface_config(
            TunnelAddresses {
                address4: "100.100.111.2".parse().unwrap(),
                address6: "fd00:0222:2021:1111::2".parse().unwrap(),
            },
            DNS_SENTINEL,
        );
        callbacks.on_tunnel_ready();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(3));
            let resources = vec![
                ResourceDescriptionCidr {
                    id: Uuid::new_v4(),
                    address: "8.8.4.4".parse::<Ipv4Addr>().unwrap().into(),
                    name: "Google Public DNS IPv4".to_string(),
                },
                ResourceDescriptionCidr {
                    id: Uuid::new_v4(),
                    address: "2001:4860:4860::8844".parse::<Ipv6Addr>().unwrap().into(),
                    name: "Google Public DNS IPv6".to_string(),
                },
            ];
            for resource in &resources {
                callbacks.on_add_route(serde_json::to_string(&resource.address).unwrap());
            }
            callbacks.on_update_resources(ResourceList {
                resources: resources
                    .into_iter()
                    .map(|resource| {
                        serde_json::to_string(&ResourceDescription::Cidr(resource)).unwrap()
                    })
                    .collect(),
            });
        });
    }

    fn disconnect_inner(runtime: &Mutex<Option<Runtime>>, callbacks: &CB, error: Option<Error>) {
        callbacks.on_disconnect(error.as_ref());

        // 1. Close the websocket connection
        // 2. Free the device handle (UNIX)
        // 3. Close the file descriptor (UNIX)
        // 4. Remove the mapping

        // The way we cleanup the tasks is we drop the runtime
        // this means we don't need to keep track of different tasks
        // but if any of the tasks never yields this will block forever!
        // So always yield and if you spawn a blocking tasks rewrite this.
        // Furthermore, we will depend on Drop impls to do the list above so,
        // implement them :)
        *runtime.lock() = None;
    }

    /// Cleanup a [Session].
    ///
    /// For now this just drops the runtime, which should drop all pending tasks.
    /// Further cleanup should be done here. (Otherwise we can just drop [Session]).
    pub fn disconnect(&mut self, error: Option<Error>) -> bool {
        Self::disconnect_inner(&self.runtime, &self.callbacks, error);
        true
    }

    /// TODO
    pub fn bump_sockets(&self) -> bool {
        true
    }

    /// TODO
    pub fn disable_some_roaming_for_broken_mobile_semantics(&self) -> bool {
        true
    }
}

fn get_websocket_path(
    mut url: Url,
    secret: String,
    mode: &str,
    public_key: &Key,
    external_id: &str,
    name_suffix: &str,
) -> Result<Url> {
    {
        let mut paths = url.path_segments_mut().map_err(|_| Error::UriError)?;
        paths.pop_if_empty();
        paths.push(mode);
        paths.push("websocket");
    }

    {
        let mut query_pairs = url.query_pairs_mut();
        query_pairs.clear();
        query_pairs.append_pair("token", &secret);
        query_pairs.append_pair("public_key", &public_key.to_string());
        query_pairs.append_pair("external_id", external_id);
        query_pairs.append_pair("name_suffix", name_suffix);
    }

    Ok(url)
}
