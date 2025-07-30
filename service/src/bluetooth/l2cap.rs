//! L2CAP socket implementation for `AirPods` communication.
//!
//! This module provides async L2CAP socket handling with separate
//! sender and receiver channels for communicating with `AirPods`.

use std::{sync::Arc, time::Duration};

use bluer::{
   Address, AddressType,
   l2cap::{SeqPacket, Socket, SocketAddr},
};
use log::{debug, warn};
use smallvec::SmallVec;
use tokio::{
   sync::{mpsc, oneshot},
   task::JoinSet,
   time,
};

use crate::error::{AirPodsError, Result};

pub type Packet = SmallVec<[u8; 32]>;

/// PSM (Protocol Service Multiplexer) for `AirPods` control channel
const PSM_CONTROL: u16 = 0x1001;
/// Maximum transmission unit for L2CAP packets
const L2CAP_MTU: usize = 672;
/// Timeout for write operations
const WRITE_TIMEOUT: Duration = Duration::from_secs(25);
/// Timeout for connection attempts
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

enum Command {
   Send {
      data: Packet,
      then: oneshot::Sender<Result<()>>,
   },
}

/// Receiver half of an L2CAP connection.
///
/// Provides async packet reception from the `AirPods` device.
#[derive(Debug)]
pub struct L2CapReceiver {
   rx: mpsc::Receiver<Result<Packet>>,
}

impl L2CapReceiver {
   pub async fn recv(&mut self) -> Result<Packet> {
      self.rx.recv().await.ok_or(AirPodsError::ConnectionClosed)?
   }
}

/// Sender half of an L2CAP connection.
///
/// Provides async packet transmission to the `AirPods` device.
/// This type is cheaply cloneable.
#[derive(Debug, Clone)]
pub struct L2CapSender {
   tx: mpsc::Sender<Command>,
}

impl L2CapSender {
   pub fn is_connected(&self) -> bool {
      !self.tx.is_closed()
   }

   pub async fn send(&self, data: &[u8]) -> Result<()> {
      if !self.is_connected() {
         return Err(AirPodsError::ConnectionClosed);
      }

      let (tx, rx) = oneshot::channel();
      self
         .tx
         .send(Command::Send {
            data: Packet::from_slice(data),
            then: tx,
         })
         .await
         .map_err(|_| AirPodsError::ConnectionClosed)?;

      time::timeout(WRITE_TIMEOUT, rx)
         .await
         .map_err(|_| AirPodsError::RequestTimeout)?
         .map_err(|_| AirPodsError::ConnectionClosed)?
   }
}

#[derive(Debug, Clone, Copy)]
pub enum HookDisposition {
   Discard,
   Retain,
}

pub struct Hooks {
   hooks: Vec<Hook>,
}

impl Hooks {
   pub const fn new() -> Self {
      Self { hooks: Vec::new() }
   }

   pub fn install(mut self, hook: Hook) -> Self {
      self.hooks.push(hook);
      self
   }
   pub fn prefix_once<F>(self, pfx: &[u8], cb: F) -> Self
   where
      F: FnOnce(&[u8]) + Send + 'static,
   {
      self.install(Hook::once(cb).prefix(pfx))
   }

   pub fn passthrough(&mut self, bytes: &Packet) {
      self
         .hooks
         .retain_mut(|hook| matches!(hook.passthrough(bytes), HookDisposition::Retain));
   }
}

pub type Callback = Box<dyn FnMut(&[u8]) + Send>;

pub struct Hook {
   pfx: heapless::Vec<u8, 8>,
   cb: Callback,
   disposition: HookDisposition,
}

impl Hook {
   pub fn once<F>(cb: F) -> Self
   where
      F: FnOnce(&[u8]) + Send + 'static,
   {
      let mut cb = Some(cb);
      Self {
         pfx: Default::default(),
         cb: Box::new(move |bytes| {
            if let Some(cb) = cb.take() {
               cb(bytes);
            }
         }),
         disposition: HookDisposition::Discard,
      }
   }

   pub fn prefix(mut self, pfx: &[u8]) -> Self {
      self.pfx = heapless::Vec::from_slice(pfx).unwrap();
      self
   }

   pub fn passthrough(&mut self, bytes: &[u8]) -> HookDisposition {
      if bytes.starts_with(&self.pfx) {
         (self.cb)(bytes);
         self.disposition
      } else {
         HookDisposition::Retain
      }
   }
}

pub async fn connect(
   jset: &mut JoinSet<()>,
   hooks: Hooks,
   address: Address,
   psm: Option<u16>,
) -> Result<(L2CapReceiver, L2CapSender)> {
   debug!("Creating L2CAP socket for {address}");

   let socket = Socket::new_seq_packet()?;
   let psm = psm.unwrap_or(PSM_CONTROL);
   let addr = SocketAddr::new(address, AddressType::BrEdr, psm);
   debug!("Connecting to {address}:{psm}");

   let seq_packet = time::timeout(CONNECT_TIMEOUT, socket.connect(addr))
      .await
      .map_err(|_| AirPodsError::RequestTimeout)??;

   let (cmd_tx, cmd_rx) = mpsc::channel(128);
   let (in_tx, in_rx) = mpsc::channel(128);

   let seq_packet = Arc::new(seq_packet);
   jset.spawn(recv_thread(address, in_tx, seq_packet.clone(), hooks));
   jset.spawn(send_thread(address, cmd_rx, seq_packet));

   Ok((L2CapReceiver { rx: in_rx }, L2CapSender { tx: cmd_tx }))
}

async fn recv_thread(
   adr: Address,
   tx: mpsc::Sender<Result<Packet>>,
   sp: Arc<SeqPacket>,
   mut hooks: Hooks,
) {
   let mut stack = [0u8; L2CAP_MTU];
   while let Ok(n) = sp.recv(&mut stack).await {
      if n == 0 {
         warn!("Connection lost");
         let _ = tx.send(Err(AirPodsError::ConnectionLost)).await;
         return;
      }
      let recvd = &stack[..n];
      debug!("← {adr}: {}", hex::encode(recvd));
      let bytes = Packet::from_slice(recvd);
      hooks.passthrough(&bytes);
      if let Err(e) = tx.send(Ok(bytes)).await {
         warn!("Failed to send data: {e:?}");
         return;
      }
      stack[..n].fill(0);
   }
}

async fn send_thread(adr: Address, mut rx: mpsc::Receiver<Command>, sp: Arc<SeqPacket>) {
   while let Some(cmd) = rx.recv().await {
      match cmd {
         Command::Send { data, then } => {
            debug!("→ {adr}: {}", hex::encode(&data));
            if let Err(e) = sp.send(&data).await {
               warn!("Failed to send data: {e}");
               let _ = then.send(Err(AirPodsError::Io(e)));
            } else {
               _ = then.send(Ok(()));
            }
         },
      }
   }
   warn!("User shutdown");
}
